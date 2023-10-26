mod message;
mod state;

use fefix::definitions::fix44;
use fefix::tagvalue::{Config, Decoder, FieldAccess, Message};
use fefix::Dictionary;
use std::pin::Pin;
use tokio::select;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, Duration, Instant, Sleep};
use tracing::{debug, error, warn};

use crate::actors::application::{ApplicationMessage, ApplicationRef};
use crate::actors::socket_writer::WriterRef;
use crate::config::SessionConfig;
use crate::message::generate_message;
use crate::message::heartbeat::Heartbeat;
use crate::message::logon::{Logon, ResetSeqNumConfig};
use crate::message::parser::RawFixMessage;
use crate::message::FixMessage;
use crate::store::MessageStore;

use message::SessionMessage;
use state::SessionState;

#[derive(Clone)]
pub struct SessionRef<M> {
    sender: mpsc::Sender<SessionMessage<M>>,
}

impl<M: FixMessage> SessionRef<M> {
    pub fn new(
        config: SessionConfig,
        application: ApplicationRef<M>,
        store: impl MessageStore + Send + Sync + 'static,
    ) -> Self {
        let (sender, mailbox) = mpsc::channel::<SessionMessage<M>>(10);
        let actor = Session::new(mailbox, config, application, store);
        tokio::spawn(run_session(actor));

        Self { sender }
    }

    pub async fn register_writer(&self, writer: WriterRef) {
        self.sender
            .send(SessionMessage::Connected(writer))
            .await
            .expect("be able to register writer");
    }

    pub async fn new_fix_message_received(&self, msg: RawFixMessage) {
        self.sender
            .send(SessionMessage::FixMessageReceived(msg))
            .await
            .expect("be able to receive message");
    }

    pub async fn disconnect(&self, reason: String) {
        self.sender
            .send(SessionMessage::Disconnected(reason))
            .await
            .expect("be able to send disconnect");
    }

    pub async fn send_message(&self, msg: M) {
        self.sender
            .send(SessionMessage::SendMessage(msg))
            .await
            .expect("message to send successfully");
    }

    pub async fn should_reconnect(&self) -> bool {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(SessionMessage::ShouldReconnect(sender))
            .await
            .unwrap();
        receiver.await.expect("to receive a response")
    }
}

struct Session<M, S> {
    mailbox: mpsc::Receiver<SessionMessage<M>>,
    config: SessionConfig,
    state: SessionState,
    application: ApplicationRef<M>,
    store: S,
    heartbeat_timer: Pin<Box<Sleep>>,
}

impl<M: FixMessage, S: MessageStore> Session<M, S> {
    fn new(
        mailbox: mpsc::Receiver<SessionMessage<M>>,
        config: SessionConfig,
        application: ApplicationRef<M>,
        store: S,
    ) -> Session<M, S> {
        let heartbeat_timer = sleep(Duration::from_secs(config.heartbeat_interval));
        Self {
            mailbox,
            config,
            state: SessionState::Disconnected {
                reconnect: true,
                reason: "initialising".to_string(),
            },
            application,
            store,
            heartbeat_timer: Box::pin(heartbeat_timer),
        }
    }

    async fn on_incoming(&mut self, message: RawFixMessage) {
        debug!("received message: {}", message);
        self.store.increment_target_seq_number().await;

        let mut decoder = Decoder::<Config>::new(Dictionary::fix44());
        let decoded_message = decoder.decode(message.as_bytes()).unwrap();
        let message_type = decoded_message.fv(fix44::MSG_TYPE).unwrap();

        match message_type {
            "0" => {
                // TODO: handle heartbeat
            }
            "1" => {
                // TODO: handle test request
            }
            "2" => {
                self.on_resend_request(&decoded_message).await;
            }
            "3" => {
                // TODO: handle reject
            }
            "4" => {
                // TODO: handle sequence reset
            }
            "5" => {
                self.on_logout().await;
            }
            "A" => {
                self.on_logon().await;
            }
            _ => {
                let parsed_message = M::parse(decoded_message);
                let app_message = ApplicationMessage::ReceivedMessage(parsed_message);
                self.application.send_message(app_message).await;
            }
        }
    }

    async fn on_connect(&mut self, writer: WriterRef) {
        self.state = SessionState::AwaitingLogon {
            writer,
            logon_sent: false,
        };
        self.send_logon().await;
    }

    async fn on_disconnect(&mut self, reason: String) {
        match self.state {
            SessionState::Active { .. } | SessionState::AwaitingLogon { .. } => {
                self.state = SessionState::Disconnected {
                    reconnect: true,
                    reason,
                }
            }
            SessionState::LoggedOut { reconnect } => {
                self.state = SessionState::Disconnected {
                    reconnect,
                    reason: "logged out".to_string(),
                }
            }
            SessionState::Disconnected { .. } => {
                warn!("disconnect message was received, but the session is already disconnected")
            }
        }
    }

    async fn on_logon(&mut self) {
        // TODO: this should check if logon message has the right sequence numbers
        // TODO: this should wait to see if a resend request is sent
        if let SessionState::AwaitingLogon { writer, .. } = &self.state {
            self.state = SessionState::Active {
                writer: writer.clone(),
            }
        } else {
            error!("received unexpected logon message");
        }
    }

    async fn on_logout(&mut self) {
        // TODO: reconnect = false isn't always valid, this should be more sophisticated
        self.state.disconnect().await;
        self.state = SessionState::LoggedOut { reconnect: false };
        self.application
            .send_logout("peer has logged us out".to_string())
            .await;
    }

    async fn on_resend_request(&mut self, _message: &Message<'_, &[u8]>) {
        // TODO: validate sequence numbers and send reject if needed
    }

    fn reset_timer(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(self.config.heartbeat_interval);
        self.heartbeat_timer.as_mut().reset(deadline);
    }

    async fn send_message(&mut self, message: impl FixMessage) {
        let seq_num = self.store.next_sender_seq_number().await;
        self.store.increment_sender_seq_number().await;

        let msg_type = message.message_type().to_vec();
        let msg = generate_message(
            &self.config.sender_comp_id,
            &self.config.target_comp_id,
            seq_num as usize,
            message,
        );
        self.state
            .send_message(&msg_type, RawFixMessage::new(msg))
            .await;
        self.reset_timer();
    }

    async fn send_logon(&mut self) {
        let reset_config = if self.config.reset_on_logon {
            self.store.reset().await;
            ResetSeqNumConfig::Reset
        } else {
            ResetSeqNumConfig::NoReset(Some(self.store.next_target_seq_number().await))
        };
        let logon = Logon::new(self.config.heartbeat_interval, reset_config);

        self.send_message(logon).await;
    }

    async fn handle(&mut self, message: SessionMessage<M>) {
        match message {
            SessionMessage::FixMessageReceived(fix_message) => {
                self.on_incoming(fix_message).await;
            }
            SessionMessage::SendHeartbeat => {
                self.send_message(Heartbeat {}).await;
            }
            SessionMessage::SendMessage(message) => {
                self.send_message(message).await;
            }
            SessionMessage::Disconnected(reason) => {
                warn!(reason, "disconnected from peer");
                self.on_disconnect(reason).await;
            }
            SessionMessage::Connected(w) => {
                self.on_connect(w).await;
            }
            SessionMessage::ShouldReconnect(responder) => {
                responder
                    .send(self.state.should_reconnect())
                    .expect("be able to respond");
            }
        }
    }
}

async fn run_session<M, S>(mut actor: Session<M, S>)
where
    M: FixMessage,
    S: MessageStore + Send + 'static,
{
    loop {
        let next_message = actor.mailbox.recv();

        select! {
            next = next_message => {
                match next {
                    Some(msg) => {
                        actor.handle(msg).await
                    }
                    None => break,
                }
            }
            () = &mut actor.heartbeat_timer.as_mut() => {
                actor.handle(SessionMessage::SendHeartbeat).await
            }
        }
    }

    debug!("session is shutting down")
}
