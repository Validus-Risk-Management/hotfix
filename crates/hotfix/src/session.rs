mod message;
mod state;

use hotfix_message::dict::Dictionary;
use hotfix_message::field_types::Timestamp;
use hotfix_message::message::{Config as MessageConfig, Message};
use hotfix_message::{fix44, FieldType, Part};
use std::cmp::Ordering;
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

use crate::error::MessageVerificationError;
use crate::message::resend_request::ResendRequest;
use crate::message::sequence_reset::SequenceReset;
use crate::message_utils::is_admin;
use crate::session::state::AwaitingResendState;
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
    message_config: MessageConfig,
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
            message_config: MessageConfig::default(),
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
        let message = Message::from_bytes(
            &self.message_config,
            &self.dictionary,
            raw_message.as_bytes(),
        );

        self.process_message(message).await;
        self.check_end_of_resend().await;
    }

    async fn process_message(&mut self, message: Message) {
        if let SessionState::AwaitingResend(state) = &mut self.state {
            let seq_number: u64 = message.header().get(fix44::MSG_SEQ_NUM).unwrap();
            if seq_number > state.end_seq_number {
                state.inbound_queue.push_back(message);
                return;
            }
        }
        // TODO: should we verify messages here?

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
                self.on_sequence_reset(&message).await;
                return; // early return as we don't need to increment target seq number
            }
            "5" => {
                self.on_logout().await;
            }
            "A" => {
                self.on_logon(&message).await;
            }
            _ => self.process_app_message(&message).await,
        }
        self.store.increment_target_seq_number().await.unwrap();
    }

    async fn process_app_message(&self, message: &Message) {
        let parsed_message = M::parse(message);
        let app_message = ApplicationMessage::ReceivedMessage(parsed_message);
        self.application.send_message(app_message).await;
    }

    async fn check_end_of_resend(&mut self) {
        let ended_state = if let SessionState::AwaitingResend(state) = &mut self.state {
            if self.store.next_target_seq_number().await > state.end_seq_number {
                let new_state = SessionState::Active {
                    writer: state.writer.clone(),
                };
                Some(std::mem::replace(&mut self.state, new_state))
            } else {
                None
            }
        } else {
            None
        };

        if let Some(SessionState::AwaitingResend(mut state)) = ended_state {
            // we have reached the end of the resend,
            // process queued messages and resume normal operation
            debug!("resend is done, processing backlog");
            while let Some(msg) = state.inbound_queue.pop_front() {
                let seq_number: u64 = msg.get(fix44::MSG_SEQ_NUM).unwrap();
                debug!(seq_number, "processing queued message");
                self.process_message(msg).await;
            }
            debug!("resend backlog is cleared, resuming normal operation");
        }
    }

    async fn verify_message(&self, message: &Message) -> Result<(), MessageVerificationError> {
        let begin_string: &str = message.header().get(fix44::BEGIN_STRING).unwrap();
        if begin_string != "FIX.4.4" {
            return Err(MessageVerificationError::IncorrectBeginString(
                begin_string.to_string(),
            ));
        }

        let expected_seq_number = self.store.next_target_seq_number().await;
        let actual_seq_number: u64 = message.header().get(fix44::MSG_SEQ_NUM).unwrap();

        match actual_seq_number.cmp(&expected_seq_number) {
            Ordering::Greater => {
                return Err(MessageVerificationError::SeqNumberTooHigh {
                    actual: actual_seq_number,
                    expected: expected_seq_number,
                });
            }
            Ordering::Less => {
                return Err(MessageVerificationError::SeqNumberTooLow {
                    actual: actual_seq_number,
                    expected: expected_seq_number,
                });
            }
            _ => {}
        }

        Ok(())
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
            SessionState::Active { .. }
            | SessionState::AwaitingLogon { .. }
            | SessionState::AwaitingResend(_) => {
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

    async fn on_logon(&mut self, message: &Message) {
        // TODO: this should wait to see if a resend request is sent
        if let SessionState::AwaitingLogon { writer, .. } = &self.state {
            match self.verify_message(message).await {
                Ok(_) => {
                    // happy logon flow, the session is now active
                    self.state = SessionState::Active {
                        writer: writer.clone(),
                    }
                }
                Err(err) => match err {
                    MessageVerificationError::SeqNumberTooLow { actual, expected } => {
                        error!("we expected {expected} sequence number, but target sent lower ({actual}), terminating...");
                        panic!("sequence number too low (actual {actual}, expected {expected})")
                    }
                    MessageVerificationError::SeqNumberTooHigh { actual, expected } => {
                        debug!("we are ahead behind target (ours: {expected}, theirs: {actual}), requesting resend.");
                        let awaiting_resend = AwaitingResendState::new(writer.to_owned(), actual);
                        self.state = SessionState::AwaitingResend(awaiting_resend);
                        self.send_resend_request(expected, actual).await;
                    }
                    MessageVerificationError::IncorrectBeginString(_) => {
                        // TODO: handle incorrect begin string/comp ID by disconnecting session
                        // see: https://www.fixtrading.org/standards/fix-session-layer-online/#when-to-terminate-a-fix-connection-by-terminating-the-transport-layer-connection-instead-of-sending-a-logout355
                        panic!("incorrect begin string received");
                    }
                    MessageVerificationError::IncorrectCompId(_) => {
                        panic!("incorrect comp ID received");
                    }
                },
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

    async fn on_sequence_reset(&mut self, message: &Message) {
        let gap_fill: bool = message.get(fix44::GAP_FILL_FLAG).unwrap();
        if !gap_fill {
            // TODO: non gap fill is valid as well of course, but I don't yet know the use-case for it is
            panic!("expected sequence reset with gap fill");
        }

        let end: u64 = message.get(fix44::NEW_SEQ_NO).unwrap();
        self.store.set_target_seq_number(end).await.unwrap();
    }

    async fn resend_messages(&mut self, begin: usize, end: usize, _message: &Message) {
        debug!(begin, end, "resending messages as requested");
        let messages = self.store.get_slice(begin, end).await.unwrap();

        let no = messages.len();
        debug!(no, "number of messages");

        let mut reset_start: Option<u64> = None;
        let mut sequence_number = 0;

        for msg in messages {
            let m = String::from_utf8(msg.clone()).unwrap();
            debug!(m, "resending message");
            let mut message =
                Message::from_bytes(&self.message_config, &self.dictionary, msg.as_slice());
            sequence_number = message.header().get(fix44::MSG_SEQ_NUM).unwrap();
            let message_type: String = message
                .header()
                .get::<&str>(fix44::MSG_TYPE)
                .unwrap()
                .to_string();

            if is_admin(message_type.as_str()) {
                debug!("skipping message as it's an admin message");
                if reset_start.is_none() {
                    reset_start = Some(sequence_number);
                }
                continue;
            }

            if let Some(begin) = reset_start {
                let end = sequence_number;
                self.send_sequence_reset(begin, end).await;
                reset_start = None;
            }

            Self::prepare_message_for_resend(&mut message);
            self.send_raw(
                message_type.as_bytes(),
                message.encode(&self.message_config),
            )
            .await;
            debug!(sequence_number, "resent message");
        }

        if let Some(begin) = reset_start {
            // the final reset if needed
            let end = sequence_number;
            self.send_sequence_reset(begin, end).await;
        }
    }

    fn prepare_message_for_resend(msg: &mut Message) {
        let header = msg.header_mut();
        let raw_sending_time = header.pop(fix44::SENDING_TIME).unwrap();
        let original_sending_time = Timestamp::deserialize(&raw_sending_time.data).unwrap();
        header.set(fix44::ORIG_SENDING_TIME, original_sending_time);
        header.set(fix44::SENDING_TIME, Timestamp::utc_now());
        header.set(fix44::POSS_DUP_FLAG, true);
    }

    fn reset_timer(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(self.config.heartbeat_interval);
        self.heartbeat_timer.as_mut().reset(deadline);
    }

    async fn send_message(&mut self, message: impl FixMessage) {
        let seq_num = self.store.next_sender_seq_number().await;
        self.store.increment_sender_seq_number().await.unwrap();

        let msg_type = message.message_type().as_bytes().to_vec();
        let msg = generate_message(
            &self.config.sender_comp_id,
            &self.config.target_comp_id,
            seq_num as usize,
            message,
        );
        self.store.add(seq_num, &msg).await.unwrap();
        self.send_raw(&msg_type, msg).await;
    }

    async fn send_raw(&mut self, message_type: &[u8], data: Vec<u8>) {
        self.state
            .send_message(message_type, RawFixMessage::new(data))
            .await;
        self.reset_timer();
    }

    async fn send_sequence_reset(&mut self, begin: u64, end: u64) {
        let sequence_reset = SequenceReset {
            gap_fill: true,
            new_seq_no: end,
        };
        let raw_message = generate_message(
            &self.config.sender_comp_id,
            &self.config.target_comp_id,
            begin as usize,
            sequence_reset,
        );

        self.send_raw(b"4", raw_message).await;
        debug!(begin, end, "sent reset sequence");
    }

    async fn send_resend_request(&mut self, begin: u64, end: u64) {
        let request = ResendRequest::new(begin, end);
        self.send_message(request).await;
    }

    async fn send_logon(&mut self) {
        let reset_config = if self.config.reset_on_logon {
            self.store.reset().await.unwrap();
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
