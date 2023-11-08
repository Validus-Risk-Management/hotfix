mod message;
mod state;

use hotfix_encoding::dict::Dictionary;
use hotfix_encoding::fix44;
use hotfix_message::message::{Config, Message};
use hotfix_message::Part;
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

use crate::message_utils::is_admin;
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
    dictionary: Dictionary,
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
            dictionary: Dictionary::fix44(),
            state: SessionState::Disconnected {
                reconnect: true,
                reason: "initialising".to_string(),
            },
            application,
            store,
            heartbeat_timer: Box::pin(heartbeat_timer),
        }
    }

    async fn on_incoming(&mut self, raw_message: RawFixMessage) {
        debug!("received message: {}", raw_message);
        self.store.increment_target_seq_number().await;

        let config = Config::default();
        let message = Message::from_bytes(config, &self.dictionary, raw_message.as_bytes());
        let message_type = message.header().get(fix44::MSG_TYPE).unwrap();

        match message_type {
            "0" => {
                // TODO: handle heartbeat
            }
            "1" => {
                // TODO: handle test request
            }
            "2" => {
                self.on_resend_request(&message).await;
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
                let parsed_message = M::parse(&message);
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

    async fn on_resend_request(&mut self, message: &Message) {
        // TODO: verify message and send reject as necessary

        let begin_seq_number: usize = match message.get(fix44::BEGIN_SEQ_NO) {
            Ok(seq_number) => seq_number,
            Err(_) => {
                // send reject if there is no valid begin number
                todo!()
            }
        };

        let end_seq_number: usize = match message.get(fix44::END_SEQ_NO) {
            Ok(seq_number) => {
                let last_seq_number = self.store.next_sender_seq_number().await as usize - 1;
                if seq_number == 0 {
                    last_seq_number
                } else {
                    std::cmp::min(seq_number, last_seq_number)
                }
            }
            Err(_) => {
                // send reject if there is no valid end number
                todo!()
            }
        };

        self.resend_messages(begin_seq_number, end_seq_number, message)
            .await;
    }

    async fn resend_messages(&self, begin: usize, end: usize, _message: &Message) {
        debug!(begin, end, "resending messages as requested");
        let messages = self.store.get_slice(begin, end).await;

        let no = messages.len();
        debug!(no, "number of messages");

        let mut reset_start: Option<u64> = None;
        let mut sequence_number = 0;

        for msg in messages {
            let m = String::from_utf8(msg.clone()).unwrap();
            debug!(m, "resending message");
            let config = Config::default();
            let message = Message::from_bytes(config, &self.dictionary, msg.as_slice());
            sequence_number = message.get(fix44::MSG_SEQ_NUM).unwrap();
            let message_type: &str = message.get(fix44::MSG_TYPE).unwrap();

            if is_admin(message_type) {
                debug!("skipping message as it's an admin message");
                if reset_start.is_none() {
                    reset_start = Some(sequence_number);
                }
                continue;
            }

            if let Some(begin) = reset_start {
                let end = sequence_number;
                debug!(begin, end, "reset sequence");
                reset_start = None;
            }

            debug!(sequence_number, "resending message");
        }

        if let Some(begin) = reset_start {
            // the final reset if needed
            let end = sequence_number;
            debug!(begin, end, "reset sequence");
        }
    }

    fn reset_timer(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(self.config.heartbeat_interval);
        self.heartbeat_timer.as_mut().reset(deadline);
    }

    async fn send_message(&mut self, message: impl FixMessage) {
        let seq_num = self.store.next_sender_seq_number().await;
        self.store.increment_sender_seq_number().await;

        let msg_type = message.message_type().as_bytes().to_vec();
        let msg = generate_message(
            &self.config.sender_comp_id,
            &self.config.target_comp_id,
            seq_num as usize,
            message,
        );
        self.store.add(seq_num, &msg).await;
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
