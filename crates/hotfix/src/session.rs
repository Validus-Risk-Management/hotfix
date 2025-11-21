mod event;
mod info;
mod session_ref;
mod state;

use crate::config::SessionConfig;
use crate::message::FixMessage;
use crate::message::generate_message;
use crate::message::heartbeat::Heartbeat;
use crate::message::logon::{Logon, ResetSeqNumConfig};
use crate::message::parser::RawFixMessage;
use crate::store::MessageStore;
use crate::transport::writer::WriterRef;
use anyhow::{Result, anyhow};
use chrono::Utc;
use hotfix_message::dict::Dictionary;
use hotfix_message::message::{Config as MessageConfig, Message};
use hotfix_message::{MessageBuilder, Part, fix44};
use std::pin::Pin;
use tokio::select;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant, Sleep, sleep, sleep_until};
use tracing::{debug, enabled, error, info, warn};

use crate::error::{CompIdType, MessageVerificationError};
use crate::message::logout::Logout;
use crate::message::resend_request::ResendRequest;
use crate::message::sequence_reset::SequenceReset;
use crate::message::test_request::TestRequest;
use crate::message_utils::{is_admin, prepare_message_for_resend};
use crate::session::state::{AwaitingResendTransitionOutcome, TestRequestId};
use crate::session_schedule::SessionSchedule;
use event::SessionEvent;
use hotfix_message::fix44::SessionRejectReason;
use hotfix_message::parsed_message::{InvalidReason, ParsedMessage};
use state::SessionState;

use crate::Application;
use crate::application::{InboundDecision, OutboundDecision};
use crate::message::reject::Reject;
use crate::message::verification::verify_message;
pub use info::{SessionInfo, Status};
pub use session_ref::SessionRef;

const SCHEDULE_CHECK_INTERVAL: u64 = 1;

struct Session<A, M, S> {
    mailbox: mpsc::Receiver<SessionEvent<M>>,
    message_config: MessageConfig,
    config: SessionConfig,
    schedule: SessionSchedule,
    message_builder: MessageBuilder,
    state: SessionState,
    application: A,
    store: S,
    schedule_check_timer: Pin<Box<Sleep>>,
}

impl<A: Application<M>, M: FixMessage, S: MessageStore> Session<A, M, S> {
    fn new(
        mailbox: mpsc::Receiver<SessionEvent<M>>,
        config: SessionConfig,
        application: A,
        store: S,
    ) -> Session<A, M, S> {
        let schedule_check_timer = sleep(Duration::from_secs(SCHEDULE_CHECK_INTERVAL));

        let dictionary = Self::get_data_dictionary(&config);
        let message_config = MessageConfig::default();
        let message_builder = MessageBuilder::new(dictionary, message_config)
            .expect("failed to create message builder");
        let schedule = config.schedule.as_ref().try_into().unwrap();

        Self {
            mailbox,
            config,
            schedule,
            message_config,
            message_builder,
            state: SessionState::new_disconnected(true, "initialising"),
            application,
            store,
            schedule_check_timer: Box::pin(schedule_check_timer),
        }
    }

    fn get_data_dictionary(config: &SessionConfig) -> Dictionary {
        match &config.data_dictionary_path {
            None => match config.begin_string.as_str() {
                "FIX.4.4" => Dictionary::fix44(),
                _ => panic!("unsupported begin string: {}", config.begin_string),
            },
            Some(dictionary_path) => Dictionary::load_from_file(dictionary_path).unwrap(),
        }
    }

    async fn on_incoming(&mut self, raw_message: RawFixMessage) -> Result<()> {
        debug!("received message: {}", raw_message);
        if !self.state.is_expecting_test_response() {
            // if we are not awaiting a specific test response, any message can reset the timer
            // otherwise only a heartbeat with the corresponding TestReqID can
            self.reset_peer_timer(None);
        }

        match self.message_builder.build(raw_message.as_bytes()) {
            ParsedMessage::Valid(message) => {
                self.process_message(message).await?;
                self.check_end_of_resend().await?;
            }
            ParsedMessage::Garbled(r) => {
                // garbled messages should be skipped and we should assume it was a transmission error
                let message = raw_message.to_string();
                let reason = format!("{r:?}");
                error!(message, reason, "received garbled message");
            }
            ParsedMessage::Invalid { message, reason } => match reason {
                InvalidReason::InvalidField(tag) | InvalidReason::InvalidGroup(tag) => {
                    match message.header().get(fix44::MSG_SEQ_NUM) {
                        Ok(msg_seq_num) => {
                            let reject = Reject::new(msg_seq_num)
                                .session_reject_reason(SessionRejectReason::InvalidTagNumber)
                                .text(&format!("invalid field {tag}"));
                            self.send_message(reject).await;
                        }
                        Err(err) => {
                            error!("failed to get message seq num: {:?}", err);
                        }
                    }
                }
                InvalidReason::InvalidComponent(_component_name) => {
                    // TODO: what's the correct way to handle this?
                    warn!("received invalid component");
                }
                InvalidReason::InvalidMsgType(msg_type) => {
                    self.handle_invalid_msg_type(message, &msg_type).await;
                }
                InvalidReason::InvalidOrderInGroup { tag, .. } => {
                    match message.header().get(fix44::MSG_SEQ_NUM) {
                        Ok(msg_seq_num) => {
                            let reject = Reject::new(msg_seq_num)
                                .session_reject_reason(
                                    SessionRejectReason::RepeatingGroupFieldsOutOfOrder,
                                )
                                .text(&format!("field appears in incorrect order:{tag}"));
                            self.send_message(reject).await;
                        }
                        Err(err) => {
                            error!("failed to get message seq num: {:?}", err);
                        }
                    }
                }
            },
            ParsedMessage::UnexpectedError(err) => {
                error!("unexpected error: {:?}", err);
            }
        }

        Ok(())
    }

    async fn process_message(&mut self, message: Message) -> Result<()> {
        let message_type = message.header().get(fix44::MSG_TYPE)?;

        if let SessionState::AwaitingResend(state) = &mut self.state {
            // TODO: consider what messages won't have a sequence number?
            // e.g. SequenceReset?
            let seq_number: u64 = message
                .header()
                .get(fix44::MSG_SEQ_NUM)
                .map_err(|e| anyhow!("failed to get seq number: {:?}", e))?;
            if seq_number > state.end_seq_number {
                state.inbound_queue.push_back(message);
                return Ok(());
            }
        }

        if let SessionState::AwaitingLogon { .. } = &mut self.state {
            // TODO: should this (and all inbound message processing) logic be pushed into the state?
            if message_type != "A" {
                self.state.disconnect().await;
                return Ok(());
            }
        }

        match message_type {
            "0" => {
                self.on_heartbeat(&message).await?;
            }
            "1" => {
                self.on_test_request(&message).await?;
            }
            "2" => {
                self.on_resend_request(&message).await?;
            }
            "3" => {
                self.on_reject(&message).await?;
            }
            "4" => {
                self.on_sequence_reset(&message).await?;
            }
            "5" => {
                self.on_logout().await?;
            }
            "A" => {
                self.on_logon(&message).await?;
            }
            _ => self.process_app_message(&message).await?,
        }

        Ok(())
    }

    async fn process_app_message(&mut self, message: &Message) -> Result<()> {
        match self.verify_message(message).await {
            Ok(_) => {
                let parsed_message = M::parse(message);
                if matches!(
                    self.application.on_inbound_message(parsed_message).await,
                    InboundDecision::TerminateSession
                ) {
                    error!("failed to send inbound message to application");
                    self.state.disconnect().await;
                }
                self.store.increment_target_seq_number().await?;
            }
            Err(err) => self.handle_verification_error(err).await,
        }

        Ok(())
    }

    async fn check_end_of_resend(&mut self) -> Result<()> {
        let ended_state = if let SessionState::AwaitingResend(state) = &mut self.state {
            if self.store.next_target_seq_number() > state.end_seq_number {
                let new_state =
                    SessionState::new_active(state.writer.clone(), self.config.heartbeat_interval);
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
                let seq_number: u64 = msg.get(fix44::MSG_SEQ_NUM).unwrap_or_else(|e| {
                    error!("failed to get seq number: {:?}", e);
                    0
                });
                debug!(seq_number, "processing queued message");
                self.process_message(msg).await?;
            }
            debug!("resend backlog is cleared, resuming normal operation");
        }

        Ok(())
    }

    async fn verify_message(
        &self,
        message: &Message,
    ) -> std::result::Result<(), MessageVerificationError> {
        verify_message(message, &self.config, self.store.next_target_seq_number())
    }

    async fn on_connect(&mut self, writer: WriterRef) {
        self.state = SessionState::AwaitingLogon {
            writer,
            logon_sent: false,
            logon_timeout: Instant::now() + Duration::from_secs(self.config.logon_timeout),
        };
        self.reset_peer_timer(None);
        self.send_logon().await;
    }

    async fn on_disconnect(&mut self, reason: String) {
        match self.state {
            SessionState::Active { .. }
            | SessionState::AwaitingLogon { .. }
            | SessionState::AwaitingResend(_) => {
                self.state.disconnect().await;
                self.state = SessionState::new_disconnected(true, &reason);
            }
            SessionState::LoggedOut { reconnect } => {
                self.state = SessionState::new_disconnected(reconnect, "logged out");
            }
            SessionState::Disconnected { .. } => {
                warn!("disconnect message was received, but the session is already disconnected")
            }
            SessionState::AwaitingLogout { .. } => {
                // this is unexpected because the other side should send a logout before disconnecting,
                // which would move this session out of the ShuttingDown state
                // TODO: is this actually true? need to review the spec carefully
                warn!("disconnect message was received, but the session is still shutting down")
            }
        }
    }

    async fn on_logon(&mut self, message: &Message) -> Result<()> {
        // TODO: this should wait to see if a resend request is sent
        if let SessionState::AwaitingLogon { writer, .. } = &self.state {
            match self.verify_message(message).await {
                Ok(_) => {
                    // happy logon flow, the session is now active
                    self.state =
                        SessionState::new_active(writer.clone(), self.config.heartbeat_interval);
                    self.application.on_logon().await;
                    self.store.increment_target_seq_number().await?;
                }
                Err(err) => self.handle_verification_error(err).await,
            }
        } else {
            error!("received unexpected logon message");
        }

        Ok(())
    }

    async fn on_logout(&mut self) -> Result<()> {
        if let SessionState::AwaitingLogout { .. } = &self.state {
            self.state.disconnect().await;
            self.state = SessionState::new_disconnected(true, "we logged out gracefully");
        } else {
            // TODO: reconnect = false isn't always valid, this should be more sophisticated
            self.state.disconnect().await;
            self.state = SessionState::LoggedOut { reconnect: false };
            self.application.on_logout("peer has logged us out").await;
        }
        self.store.increment_target_seq_number().await
    }

    async fn on_heartbeat(&mut self, message: &Message) -> Result<()> {
        if let (Some(expected_req_id), Ok(message_req_id)) = (
            &self.state.expected_test_response_id(),
            message.get::<&str>(fix44::TEST_REQ_ID),
        ) && expected_req_id.as_str() == message_req_id
        {
            debug!("received response for TestRequest, resetting timer");
            self.reset_peer_timer(None);
        }

        self.store.increment_target_seq_number().await
    }

    async fn on_test_request(&mut self, message: &Message) -> Result<()> {
        let req_id: &str = message.get(fix44::TEST_REQ_ID).unwrap_or_else(|_| {
            // TODO: send reject?
            todo!()
        });

        self.store.increment_target_seq_number().await?;

        self.send_message(Heartbeat::for_request(req_id.to_string()))
            .await;

        Ok(())
    }

    async fn on_resend_request(&mut self, message: &Message) -> Result<()> {
        if !self.state.is_connected() {
            warn!("received resend request while disconnected, ignoring");
        }

        let begin_seq_number: u64 = match message.get(fix44::BEGIN_SEQ_NO) {
            Ok(seq_number) => seq_number,
            Err(_) => {
                let reject = Reject::new(
                    message
                        .header()
                        .get(fix44::MSG_SEQ_NUM)
                        .map_err(|_| anyhow!("failed to get seq number"))?,
                )
                .session_reject_reason(SessionRejectReason::RequiredTagMissing)
                .text("missing begin sequence number for resend request");
                self.send_message(reject).await;
                return Ok(());
            }
        };

        let end_seq_number: u64 = match message.get(fix44::END_SEQ_NO) {
            Ok(seq_number) => {
                let last_seq_number = self.store.next_sender_seq_number() - 1;
                if seq_number == 0 {
                    last_seq_number
                } else {
                    std::cmp::min(seq_number, last_seq_number)
                }
            }
            Err(_) => {
                let reject = Reject::new(
                    message
                        .header()
                        .get(fix44::MSG_SEQ_NUM)
                        .map_err(|_| anyhow!("failed to get seq number"))?,
                )
                .session_reject_reason(SessionRejectReason::RequiredTagMissing)
                .text("missing end sequence number for resend request");
                self.send_message(reject).await;
                return Ok(());
            }
        };

        self.store.increment_target_seq_number().await?;

        self.resend_messages(begin_seq_number, end_seq_number, message)
            .await;

        Ok(())
    }

    /// Handle Reject messages.
    ///
    /// Returns whether the message should be processed as usual
    /// and whether the target sequence number should be incremented.
    async fn on_reject(&mut self, message: &Message) -> Result<()> {
        if let Ok(seq_num) = message.get::<u64>(fix44::MSG_SEQ_NUM)
            && seq_num == self.store.next_target_seq_number()
        {
            self.store.increment_target_seq_number().await?;
        }

        Ok(())
    }

    async fn on_sequence_reset(&mut self, message: &Message) -> Result<()> {
        let gap_fill: bool = message.get(fix44::GAP_FILL_FLAG).unwrap();
        if !gap_fill {
            // TODO: non gap fill is valid as well of course, but I don't yet know the use-case for it is
            panic!("expected sequence reset with gap fill");
        }

        let end: u64 = message.get(fix44::NEW_SEQ_NO).unwrap();
        self.store.set_target_seq_number(end - 1).await
    }

    async fn handle_verification_error(&mut self, error: MessageVerificationError) {
        match error {
            MessageVerificationError::SeqNumberTooLow {
                expected,
                actual,
                possible_duplicate,
            } => {
                self.handle_sequence_number_too_low(expected, actual, possible_duplicate)
                    .await;
            }
            MessageVerificationError::SeqNumberTooHigh { expected, actual } => {
                self.handle_sequence_number_too_high(expected, actual).await;
            }
            MessageVerificationError::IncorrectBeginString(begin_string) => {
                self.handle_incorrect_begin_string(begin_string).await;
            }
            MessageVerificationError::IncorrectCompId {
                comp_id,
                comp_id_type,
                msg_seq_num,
            } => {
                self.handle_incorrect_comp_id(comp_id, comp_id_type, msg_seq_num)
                    .await;
            }
            MessageVerificationError::SendingTimeAccuracyIssue { msg_seq_num } => {
                self.handle_sending_time_accuracy_problem(msg_seq_num, "unexpected sending time")
                    .await;
            }
            MessageVerificationError::SendingTimeMissing { msg_seq_num } => {
                self.handle_sending_time_accuracy_problem(msg_seq_num, "sending time missing")
                    .await;
            }
            MessageVerificationError::OriginalSendingTimeMissing { msg_seq_num } => {
                self.handle_original_sending_time_missing(msg_seq_num).await;
            }
            MessageVerificationError::OriginalSendingTimeAfterSendingTime {
                msg_seq_num, ..
            } => {
                self.handle_sending_time_accuracy_problem(
                    msg_seq_num,
                    "original sending time is after sending time",
                )
                .await;
            }
        }
    }

    async fn handle_incorrect_begin_string(&mut self, received_begin_string: String) {
        self.logout_and_terminate(&format!(
            "beginString={received_begin_string} is not supported"
        ))
        .await;
    }

    async fn handle_incorrect_comp_id(
        &mut self,
        received_comp_id: String,
        comp_id_type: CompIdType,
        msg_seq_num: u64,
    ) {
        error!(
            "rejecting message with incorrect comp ID: {received_comp_id} (type: {comp_id_type:?})"
        );
        let reject = Reject::new(msg_seq_num)
            .session_reject_reason(SessionRejectReason::ValueIsIncorrect)
            .text(&format!("invalid comp ID {received_comp_id}"));
        self.send_message(reject).await;

        self.logout_and_terminate("incorrect comp ID received")
            .await;
    }

    async fn handle_sequence_number_too_low(
        &mut self,
        expected: u64,
        actual: u64,
        possible_duplicate: bool,
    ) {
        if possible_duplicate {
            warn!(
                "sequence number too low (expected {expected}, actual {actual}, but counterparty indicated it's poss duplicate, ignoring"
            );
            return;
        }
        error!(
            "we expected {expected} sequence number, but target sent lower ({actual}), terminating..."
        );
        let reason = format!("sequence number too low (actual {actual}, expected {expected})");
        self.logout_and_terminate(&reason).await;
        self.state = SessionState::LoggedOut { reconnect: false };
    }

    async fn handle_sequence_number_too_high(&mut self, expected: u64, actual: u64) {
        match self
            .state
            .try_transition_to_awaiting_resend(expected, actual)
        {
            AwaitingResendTransitionOutcome::Success => {
                debug!(
                    "we are behind target (ours: {expected}, theirs: {actual}), requesting resend."
                );
                self.send_resend_request(expected, actual).await;
            }
            AwaitingResendTransitionOutcome::InvalidState(reason) => {
                error!("failed to request resend: {reason}");
            }
            AwaitingResendTransitionOutcome::BeginSeqNumberTooLow => {
                self.state.disconnect().await;
                self.state = SessionState::new_disconnected(
                    false,
                    "awaiting resend begin seq number unexpectedly lower than the previous resend request's",
                );
            }
            AwaitingResendTransitionOutcome::AttemptsExceeded => {
                self.state.disconnect().await;
                self.state = SessionState::new_disconnected(
                    false,
                    "resend request attempts exceeded, manual intervention required",
                );
            }
        }
    }

    async fn handle_invalid_msg_type(&mut self, message: Message, msg_type: &str) {
        match message.header().get(fix44::MSG_SEQ_NUM) {
            Ok(msg_seq_num) => {
                let reject = Reject::new(msg_seq_num)
                    .session_reject_reason(SessionRejectReason::InvalidMsgtype)
                    .text(&format!("invalid message type {msg_type}"));
                self.send_message(reject).await;

                #[allow(clippy::collapsible_if)]
                if let Ok(seq_num) = message.header().get::<u64>(fix44::MSG_SEQ_NUM)
                    && self.store.next_target_seq_number() == seq_num
                {
                    if let Err(err) = self.store.increment_target_seq_number().await {
                        error!("failed to increment target seq number: {:?}", err);
                    };
                }
            }
            Err(err) => {
                error!("failed to get message seq num: {:?}", err);
            }
        }
    }

    async fn handle_sending_time_accuracy_problem(&mut self, msg_seq_num: u64, text: &str) {
        let reject = Reject::new(msg_seq_num)
            .session_reject_reason(SessionRejectReason::SendingtimeAccuracyProblem)
            .text(text);
        self.send_message(reject).await;
        if let Err(err) = self.store.increment_target_seq_number().await {
            error!("failed to increment target seq number: {:?}", err);
        };
    }

    async fn handle_original_sending_time_missing(&mut self, msg_seq_num: u64) {
        let reject = Reject::new(msg_seq_num)
            .session_reject_reason(SessionRejectReason::RequiredTagMissing)
            .text("original sending time is required");
        self.send_message(reject).await;
        if let Err(err) = self.store.increment_target_seq_number().await {
            error!("failed to increment target seq number: {:?}", err);
        };
    }

    async fn resend_messages(&mut self, begin: u64, end: u64, _message: &Message) {
        info!(begin, end, "resending messages as requested");
        let messages = self
            .store
            .get_slice(begin as usize, end as usize)
            .await
            .unwrap();

        let no = messages.len();
        debug!(number_of_messages = no, "number of messages");

        let mut reset_start: Option<u64> = None;
        let mut sequence_number = 0;

        for msg in messages {
            let mut message = self
                .message_builder
                .build(msg.as_slice())
                .into_message()
                .unwrap();
            sequence_number = message.header().get(fix44::MSG_SEQ_NUM).unwrap();
            let message_type: String = message
                .header()
                .get::<&str>(fix44::MSG_TYPE)
                .unwrap()
                .to_string();

            if is_admin(message_type.as_str()) {
                if reset_start.is_none() {
                    reset_start = Some(sequence_number);
                }
                continue;
            }

            if let Some(begin) = reset_start {
                let end = sequence_number;
                Self::log_skipped_admin_messages(begin, end);
                self.send_sequence_reset(begin, end).await;
                reset_start = None;
            }

            if let Err(e) = prepare_message_for_resend(&mut message) {
                error!(
                    error = e,
                    "failed to prepare message for resend, sending original"
                );
            }
            self.send_raw(
                message_type.as_bytes(),
                message.encode(&self.message_config).unwrap(),
            )
            .await;

            if enabled!(tracing::Level::DEBUG) {
                let m = String::from_utf8(msg.clone()).unwrap();
                debug!(sequence_number, message = m, "resent message");
            }
        }

        if let Some(begin) = reset_start {
            // the final reset if needed
            let end = sequence_number;
            Self::log_skipped_admin_messages(begin, end);
            self.send_sequence_reset(begin, end).await;
        }
    }

    fn log_skipped_admin_messages(begin: u64, end: u64) {
        info!(
            begin,
            end, "skipped admin message(s) during resend, requesting reset for these"
        );
    }

    fn reset_heartbeat_timer(&mut self) {
        self.state
            .reset_heartbeat_timer(self.config.heartbeat_interval);
    }

    fn reset_peer_timer(&mut self, test_request_id: Option<TestRequestId>) {
        self.state
            .reset_peer_timer(self.config.heartbeat_interval, test_request_id);
    }

    async fn send_app_message(&mut self, message: M) {
        match self.application.on_outbound_message(&message).await {
            OutboundDecision::Send => {
                self.send_message(message).await;
            }
            OutboundDecision::Drop => {
                debug!("dropped outbound message as instructed by the application");
            }
            OutboundDecision::TerminateSession => {
                error!("failed to send message to application");
                self.state.disconnect().await;
            }
        }
    }

    async fn send_message(&mut self, message: impl FixMessage) {
        let seq_num = self.store.next_sender_seq_number();
        self.store.increment_sender_seq_number().await.unwrap();

        let msg_type = message.message_type().as_bytes().to_vec();
        let msg = generate_message(
            &self.config.begin_string,
            &self.config.sender_comp_id,
            &self.config.target_comp_id,
            seq_num,
            message,
        )
        .unwrap();
        self.store.add(seq_num, &msg).await.unwrap();
        self.send_raw(&msg_type, msg).await;
    }

    async fn send_raw(&mut self, message_type: &[u8], data: Vec<u8>) {
        self.state
            .send_message(message_type, RawFixMessage::new(data))
            .await;
        self.reset_heartbeat_timer();
    }

    async fn send_sequence_reset(&mut self, begin: u64, end: u64) {
        let sequence_reset = SequenceReset {
            gap_fill: true,
            new_seq_no: end,
        };
        let raw_message = generate_message(
            &self.config.begin_string,
            &self.config.sender_comp_id,
            &self.config.target_comp_id,
            begin,
            sequence_reset,
        )
        .unwrap();

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
            ResetSeqNumConfig::NoReset(Some(self.store.next_target_seq_number()))
        };
        let logon = Logon::new(self.config.heartbeat_interval, reset_config);

        self.send_message(logon).await;
    }

    async fn logout(&mut self, reason: &str) {
        let logout = Logout::with_reason(reason.to_string());
        self.send_message(logout).await;
    }

    async fn logout_and_terminate(&mut self, reason: &str) {
        self.logout(reason).await;
        self.state.disconnect().await;
    }

    async fn initiate_graceful_logout(&mut self, reason: &str) {
        if self.state.try_transition_to_awaiting_logout() {
            self.logout(reason).await;
        }
    }

    async fn handle(&mut self, event: SessionEvent<M>) {
        self.handle_schedule_check().await;

        match event {
            SessionEvent::FixMessageReceived(fix_message) => {
                if let Err(err) = self.on_incoming(fix_message).await {
                    let reason = err.to_string();
                    error!(reason, "fatal error in message processing");
                    self.logout_and_terminate("internal error").await;
                }
            }
            SessionEvent::SendMessage(message) => {
                self.send_app_message(message).await;
            }
            SessionEvent::Disconnected(reason) => {
                warn!(reason, "disconnected from peer");
                self.on_disconnect(reason).await;
            }
            SessionEvent::Connected(w) => {
                self.on_connect(w).await;
            }
            SessionEvent::ShouldReconnect(responder) => {
                responder
                    .send(self.state.should_reconnect())
                    .expect("be able to respond");
            }
            SessionEvent::AwaitingActiveSession(responder) => {
                self.state.register_session_awaiter(responder);
            }
            SessionEvent::SessionInfoRequested(responder) => {
                if responder.send(self.get_session_info()).is_err() {
                    error!("failed to respond to session info request");
                }
            }
            SessionEvent::ShutdownRequested => {
                // TODO: revisit logout & shutdown flows once logout timeouts are implemented
                self.logout_and_terminate("shutdown requested").await;
                self.state = SessionState::new_disconnected(false, "shutdown requested");
            }
        }
    }

    async fn handle_heartbeat_timeout(&mut self) {
        self.send_message(Heartbeat::default()).await;
    }

    async fn handle_peer_timeout(&mut self) {
        if self.state.is_expecting_test_response() {
            warn!("peer didn't respond, terminating..");
            self.logout_and_terminate("peer timeout").await;
        } else if self.state.is_awaiting_logon() {
            warn!("peer didn't respond to our Logon, disconnecting..");
            self.state.disconnect().await;
        } else {
            let req_id = format!("TEST_{}", self.store.next_target_seq_number());
            info!("sending TestRequest due to peer timer expiring");
            let request = TestRequest::new(req_id.clone());
            self.send_message(request).await;
            self.reset_peer_timer(Some(req_id));
        }
    }

    async fn handle_schedule_check(&mut self) {
        let now = Utc::now();
        let is_active = self.schedule.is_active_at(&now);

        if is_active {
            self.state.notify_session_awaiter();
            match self
                .schedule
                .is_same_session_period(&self.store.creation_time(), &now)
            {
                Ok(true) => {
                    // we are in the same period, nothing needs to be done
                }
                Ok(false) => {
                    // the message store is for a previous session,
                    // we need to terminate this session, reset the store, and reestablish the session
                    self.logout_and_terminate("session period changed").await;
                    if let Err(err) = self.store.reset().await {
                        error!("error resetting session store: {err:}");
                        self.state =
                            SessionState::new_disconnected(false, "unexpected error in reset");
                    }
                }
                Err(err) => {
                    error!("error checking session period: {err:?}");
                    self.logout_and_terminate("internal error").await;
                }
            }
        } else if self.state.is_connected() {
            // we are currently outside scheduled session time
            self.initiate_graceful_logout("End of session time").await;
        }

        // we always need to reschedule the check, otherwise we won't be able to resume an inactive session
        let deadline = Instant::now() + Duration::from_secs(SCHEDULE_CHECK_INTERVAL);
        self.schedule_check_timer.as_mut().reset(deadline);
    }

    fn get_session_info(&self) -> SessionInfo {
        SessionInfo {
            next_sender_seq_number: self.store.next_sender_seq_number(),
            next_target_seq_number: self.store.next_target_seq_number(),
            status: self.state.as_status(),
        }
    }
}

async fn run_session<A, M, S>(mut session: Session<A, M, S>)
where
    A: Application<M>,
    M: FixMessage,
    S: MessageStore + Send + 'static,
{
    loop {
        let next_message = session.mailbox.recv();

        select! {
            next = next_message => {
                match next {
                    Some(msg) => {
                        session.handle(msg).await
                    }
                    None => break,
                }
            }
            () = async {
                if let Some(deadline) = session.state.heartbeat_deadline() {
                    sleep_until(*deadline).await
                } else {
                    std::future::pending().await
                }
            } => {
                session.handle_heartbeat_timeout().await;
            }
            () = async {
                if let Some(deadline) = session.state.peer_deadline() {
                    sleep_until(*deadline).await
                } else {
                    std::future::pending().await
                }
            } => {
                session.handle_peer_timeout().await;
            }
            () = &mut session.schedule_check_timer.as_mut() => {
                session.handle_schedule_check().await;
            }
        }
    }

    debug!("session is shutting down")
}
