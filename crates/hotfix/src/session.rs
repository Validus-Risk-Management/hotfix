pub(crate) mod admin_request;
mod ctx;
pub mod error;
pub(crate) mod event;
mod info;
mod outbound;
mod session_handle;
pub mod session_ref;
mod state;

use chrono::Utc;
use hotfix_message::dict::Dictionary;
use hotfix_message::message::{Config as MessageConfig, Message};
use hotfix_message::{MessageBuilder, Part};
use std::pin::Pin;
use tokio::select;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant, Sleep, sleep, sleep_until};
use tracing::{debug, error, info, warn};

use crate::Application;
use crate::application::{InboundDecision, OutboundDecision};
use crate::config::SessionConfig;
use crate::message::OutboundMessage;
use crate::message::business_reject::BusinessReject;
use crate::message::heartbeat::Heartbeat;
use crate::message::logon::{Logon, ResetSeqNumConfig};
use crate::message::logout::Logout;
use crate::message::parser::RawFixMessage;
use crate::message::reject::Reject;
use crate::message::resend_request::ResendRequest;
use crate::message::sequence_reset::SequenceReset;
use crate::message::test_request::TestRequest;
use crate::message::verification::verify_message;
use crate::message::verification_error::{CompIdType, MessageVerificationError};
use crate::session::admin_request::AdminRequest;
use crate::session::ctx::SessionCtx;
use crate::session::error::SessionCreationError;
use crate::session::error::{InternalSendError, InternalSendResultExt, SessionOperationError};
pub use crate::session::error::{SendError, SendOutcome};
pub use crate::session::info::{SessionInfo, Status};
pub use crate::session::session_handle::SessionHandle;
#[cfg(not(feature = "test-utils"))]
pub(crate) use crate::session::session_ref::InternalSessionRef;
#[cfg(feature = "test-utils")]
pub use crate::session::session_ref::InternalSessionRef;
use crate::session::session_ref::OutboundRequest;
use crate::session::state::SessionState;
use crate::session::state::{
    AwaitingLogonState, AwaitingLogoutState, AwaitingResendTransitionOutcome, TestRequestId,
};
use crate::session_schedule::{SessionPeriodComparison, SessionSchedule};
use crate::store::MessageStore;
use crate::transport::writer::WriterRef;
use event::SessionEvent;
use hotfix_message::parsed_message::{InvalidReason, ParsedMessage};
use hotfix_message::session_fields::{
    BEGIN_SEQ_NO, END_SEQ_NO, GAP_FILL_FLAG, MSG_SEQ_NUM, MSG_TYPE, NEW_SEQ_NO,
    SessionRejectReason, TEST_REQ_ID,
};

const SCHEDULE_CHECK_INTERVAL: u64 = 1;

struct Session<A, S> {
    ctx: SessionCtx<A, S>,
    schedule: SessionSchedule,
    state: SessionState,
    schedule_check_timer: Pin<Box<Sleep>>,
    reset_on_next_logon: bool,
}

impl<App, Store> Session<App, Store>
where
    App: Application,
    Store: MessageStore,
{
    fn new(
        config: SessionConfig,
        application: App,
        store: Store,
    ) -> Result<Session<App, Store>, SessionCreationError> {
        let schedule_check_timer = sleep(Duration::from_secs(SCHEDULE_CHECK_INTERVAL));

        let dictionary = Self::get_data_dictionary(&config)?;
        let message_config = MessageConfig::default();
        let message_builder = MessageBuilder::new(dictionary, message_config)?;
        let schedule = config.schedule.as_ref().try_into()?;
        let ctx = SessionCtx {
            config,
            store,
            application,
            message_builder,
            message_config,
        };

        let session = Self {
            ctx,
            schedule,
            state: SessionState::new_disconnected(true, "initialising"),
            schedule_check_timer: Box::pin(schedule_check_timer),
            reset_on_next_logon: false,
        };

        Ok(session)
    }

    fn get_data_dictionary(config: &SessionConfig) -> Result<Dictionary, SessionCreationError> {
        match &config.data_dictionary_path {
            None => match config.begin_string.as_str() {
                #[cfg(feature = "fix44")]
                "FIX.4.4" => Ok(Dictionary::fix44()),
                _ => Err(SessionCreationError::UnsupportedBeginString(
                    config.begin_string.to_string(),
                )),
            },
            Some(dictionary_path) => Ok(Dictionary::load_from_file(dictionary_path)?),
        }
    }

    async fn on_incoming(
        &mut self,
        raw_message: RawFixMessage,
    ) -> Result<(), SessionOperationError> {
        debug!("received message: {}", raw_message);
        if !self.state.is_expecting_test_response() {
            // if we are not awaiting a specific test response, any message can reset the timer
            // otherwise only a heartbeat with the corresponding TestReqID can
            self.reset_peer_timer(None);
        }

        match self.ctx.message_builder.build(raw_message.as_bytes()) {
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
                    match message.header().get(MSG_SEQ_NUM) {
                        Ok(msg_seq_num) => {
                            let reject = Reject::new(msg_seq_num)
                                .session_reject_reason(SessionRejectReason::InvalidTagNumber)
                                .text(&format!("invalid field {tag}"));
                            self.send_message(reject)
                                .await
                                .with_send_context("reject for invalid field")?;
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
                    match message.header().get(MSG_SEQ_NUM) {
                        Ok(msg_seq_num) => {
                            let reject = Reject::new(msg_seq_num)
                                .session_reject_reason(
                                    SessionRejectReason::RepeatingGroupFieldsOutOfOrder,
                                )
                                .text(&format!("field appears in incorrect order:{tag}"));
                            self.send_message(reject)
                                .await
                                .with_send_context("reject for invalid group order")?;
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

    async fn process_message(&mut self, message: Message) -> Result<(), SessionOperationError> {
        let message_type: &str = message
            .header()
            .get(MSG_TYPE)
            .map_err(|_| SessionOperationError::MissingField("MSG_TYPE"))?;

        if let SessionState::AwaitingResend(state) = &mut self.state {
            let seq_number = get_msg_seq_num(&message);
            if seq_number > state.end_seq_number && message_type != ResendRequest::MSG_TYPE {
                state.inbound_queue.push_back(message);
                return Ok(());
            }
        }

        if let SessionState::AwaitingLogon(_) = &mut self.state {
            // TODO: should this (and all inbound message processing) logic be pushed into the state?
            if message_type != Logon::MSG_TYPE {
                self.state.disconnect_writer().await;
                return Ok(());
            }
        }

        match message_type {
            Heartbeat::MSG_TYPE => {
                self.on_heartbeat(&message).await?;
            }
            TestRequest::MSG_TYPE => {
                self.on_test_request(&message).await?;
            }
            ResendRequest::MSG_TYPE => {
                self.on_resend_request(&message).await?;
            }
            Reject::MSG_TYPE => {
                self.on_reject(&message).await?;
            }
            SequenceReset::MSG_TYPE => {
                self.on_sequence_reset(&message).await?;
            }
            Logout::MSG_TYPE => {
                self.on_logout(&message).await?;
            }
            Logon::MSG_TYPE => {
                self.on_logon(&message).await?;
            }
            _ => self.process_app_message(&message).await?,
        }

        Ok(())
    }

    async fn process_app_message(
        &mut self,
        message: &Message,
    ) -> Result<(), SessionOperationError> {
        match self.verify_message(message, true, true) {
            Ok(_) => {
                match self.ctx.application.on_inbound_message(message).await {
                    InboundDecision::Accept => {}
                    InboundDecision::Reject { reason, text } => {
                        let msg_type: &str = message
                            .header()
                            .get(MSG_TYPE)
                            .map_err(|_| SessionOperationError::MissingField("MSG_TYPE"))?;
                        let mut reject = BusinessReject::new(msg_type, reason)
                            .ref_seq_num(get_msg_seq_num(message));
                        if let Some(text) = text {
                            reject = reject.text(&text);
                        }
                        self.send_message(reject)
                            .await
                            .with_send_context("business message reject")?;
                    }
                    InboundDecision::TerminateSession => {
                        error!("failed to send inbound message to application");
                        self.state.disconnect_writer().await;
                    }
                }
                self.ctx.store.increment_target_seq_number().await?;
            }
            Err(err) => self.handle_verification_error(err).await?,
        }

        Ok(())
    }

    async fn check_end_of_resend(&mut self) -> Result<(), SessionOperationError> {
        let ended_state = if let SessionState::AwaitingResend(state) = &mut self.state {
            if self.ctx.store.next_target_seq_number() > state.end_seq_number {
                let new_state = SessionState::new_active(
                    state.writer.clone(),
                    self.ctx.config.heartbeat_interval,
                );
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
                let seq_number: u64 = msg.get(MSG_SEQ_NUM).unwrap_or_else(|e| {
                    error!("failed to get seq number: {:?}", e);
                    0
                });
                let msg_type: &str = msg.header().get(MSG_TYPE).unwrap_or("");
                debug!(seq_number, msg_type, "processing queued message");

                if msg_type == ResendRequest::MSG_TYPE {
                    // ResendRequest was already processed when it arrived (it bypasses
                    // the queue in process_message). Just increment the target seq number
                    // for sequence accounting purposes.
                    self.ctx.store.increment_target_seq_number().await?;
                } else {
                    self.process_message(msg).await?;
                }
            }
            debug!("resend backlog is cleared, resuming normal operation");
        }

        Ok(())
    }

    fn verify_message(
        &self,
        message: &Message,
        check_too_high: bool,
        check_too_low: bool,
    ) -> Result<(), MessageVerificationError> {
        let expected_seq_number = if check_too_high || check_too_low {
            Some(self.ctx.store.next_target_seq_number())
        } else {
            None
        };
        verify_message(
            message,
            &self.ctx.config,
            expected_seq_number,
            check_too_high,
            check_too_low,
        )
    }

    async fn on_connect(&mut self, writer: WriterRef) -> Result<(), SessionOperationError> {
        self.state = SessionState::AwaitingLogon(AwaitingLogonState {
            writer,
            logon_sent: false,
            logon_timeout: Instant::now() + Duration::from_secs(self.ctx.config.logon_timeout),
        });
        self.reset_peer_timer(None);
        self.send_logon().await?;

        Ok(())
    }

    async fn on_disconnect(&mut self, reason: String) {
        match self.state {
            SessionState::Active(_)
            | SessionState::AwaitingLogon(_)
            | SessionState::AwaitingResend(_) => {
                self.state.disconnect_writer().await;
                self.state = SessionState::new_disconnected(true, &reason);
            }
            SessionState::Disconnected(_) => {
                warn!("disconnect message was received, but the session is already disconnected")
            }
            SessionState::AwaitingLogout(AwaitingLogoutState { reconnect, .. }) => {
                self.state = SessionState::new_disconnected(reconnect, &reason);
            }
        }
    }

    async fn on_logon(&mut self, message: &Message) -> Result<(), SessionOperationError> {
        if let SessionState::AwaitingLogon(AwaitingLogonState { writer, .. }) = &self.state {
            match self.verify_message(message, true, true) {
                Ok(_) => {
                    // happy logon flow, the session is now active
                    self.state = SessionState::new_active(
                        writer.clone(),
                        self.ctx.config.heartbeat_interval,
                    );
                    self.ctx.application.on_logon().await;
                    self.ctx.store.increment_target_seq_number().await?;
                }
                Err(err) => self.handle_verification_error(err).await?,
            }
        } else {
            error!("received unexpected logon message");
        }

        Ok(())
    }

    async fn on_logout(&mut self, message: &Message) -> Result<(), SessionOperationError> {
        if let Err(err) = self.verify_message(message, false, false) {
            self.handle_verification_error(err).await?;
            return Ok(());
        }

        if self.state.is_logged_on() {
            self.send_logout("Logout acknowledged").await?;
        }

        self.ctx
            .application
            .on_logout("peer has logged us out")
            .await;

        match self.state {
            // if the session is already disconnected, we have nothing else to do
            SessionState::Disconnected(..) => {}
            // if we initiated the logout, preserve the reconnect flag
            SessionState::AwaitingLogout(AwaitingLogoutState { reconnect, .. }) => {
                self.state.disconnect_writer().await;
                self.state = SessionState::new_disconnected(reconnect, "logout completed");
            }
            // otherwise assume it makes sense to try to reconnect
            _ => {
                self.state.disconnect_writer().await;
                self.state = SessionState::new_disconnected(true, "peer has logged us out")
            }
        }

        self.ctx.store.increment_target_seq_number().await?;
        Ok(())
    }

    async fn on_heartbeat(&mut self, message: &Message) -> Result<(), SessionOperationError> {
        if let Err(err) = self.verify_message(message, true, true) {
            self.handle_verification_error(err).await?;
            return Ok(());
        }

        if let (Some(expected_req_id), Ok(message_req_id)) = (
            &self.state.expected_test_response_id(),
            message.get::<&str>(TEST_REQ_ID),
        ) && expected_req_id.as_str() == message_req_id
        {
            debug!("received response for TestRequest, resetting timer");
            self.reset_peer_timer(None);
        }

        self.ctx.store.increment_target_seq_number().await?;
        Ok(())
    }

    async fn on_test_request(&mut self, message: &Message) -> Result<(), SessionOperationError> {
        if let Err(err) = self.verify_message(message, true, true) {
            self.handle_verification_error(err).await?;
            return Ok(());
        }

        let req_id: &str = message.get(TEST_REQ_ID).unwrap_or_else(|_| {
            // TODO: send reject?
            todo!()
        });

        self.ctx.store.increment_target_seq_number().await?;

        self.send_message(Heartbeat::for_request(req_id.to_string()))
            .await
            .with_send_context("heartbeat response")?;

        Ok(())
    }

    async fn on_resend_request(&mut self, message: &Message) -> Result<(), SessionOperationError> {
        if !self.state.is_connected() {
            warn!("received resend request while disconnected, ignoring");
            return Ok(());
        }

        // Verify with check_too_high=false so ResendRequest is never blocked by seq-too-high.
        // This is the key part of the QFJ-673 deadlock fix: when both sides send ResendRequest
        // simultaneously, each side's ResendRequest will have a seq number higher than expected.
        // By not treating that as an error, we allow the ResendRequest to be processed.
        match self.verify_message(message, false, true) {
            Ok(_) => {}
            Err(err) => {
                self.handle_verification_error(err).await?;
                return Ok(());
            }
        }

        let msg_seq_num = get_msg_seq_num(message);
        let expected = self.ctx.store.next_target_seq_number();

        // If seq is too high and we're in AwaitingResend, queue it for seq accounting
        // when the gap fill catches up, but still process the resend below.
        if msg_seq_num > expected
            && let SessionState::AwaitingResend(state) = &mut self.state
        {
            state.inbound_queue.push_back(message.clone());
        }

        let begin_seq_number: u64 = match message.get(BEGIN_SEQ_NO) {
            Ok(seq_number) => seq_number,
            Err(_) => {
                let reject = Reject::new(msg_seq_num)
                    .session_reject_reason(SessionRejectReason::RequiredTagMissing)
                    .text("missing begin sequence number for resend request");
                self.send_message(reject)
                    .await
                    .with_send_context("reject for missing BEGIN_SEQ_NO")?;
                return Ok(());
            }
        };

        let end_seq_number: u64 = match message.get(END_SEQ_NO) {
            Ok(seq_number) => {
                let last_seq_number = self.ctx.store.next_sender_seq_number() - 1;
                if seq_number == 0 {
                    last_seq_number
                } else {
                    std::cmp::min(seq_number, last_seq_number)
                }
            }
            Err(_) => {
                let reject = Reject::new(msg_seq_num)
                    .session_reject_reason(SessionRejectReason::RequiredTagMissing)
                    .text("missing end sequence number for resend request");
                self.send_message(reject)
                    .await
                    .with_send_context("reject for missing END_SEQ_NO")?;
                return Ok(());
            }
        };

        // Only increment target seq if seq matches expected
        if msg_seq_num == expected {
            self.ctx.store.increment_target_seq_number().await?;
        }

        if let Some(writer) = self.state.get_writer() {
            outbound::resend_messages(&mut self.ctx, writer, begin_seq_number, end_seq_number)
                .await?;
            self.reset_heartbeat_timer();
        }

        Ok(())
    }

    /// Handle Reject messages.
    async fn on_reject(&mut self, message: &Message) -> Result<(), SessionOperationError> {
        if let Err(err) = self.verify_message(message, false, true) {
            self.handle_verification_error(err).await?;
            return Ok(());
        }

        self.ctx.store.increment_target_seq_number().await?;
        Ok(())
    }

    async fn on_sequence_reset(&mut self, message: &Message) -> Result<(), SessionOperationError> {
        let msg_seq_num = get_msg_seq_num(message);
        let is_gap_fill: bool = message.get(GAP_FILL_FLAG).unwrap_or(false);
        if let Err(err) = self.verify_message(message, is_gap_fill, is_gap_fill) {
            self.handle_verification_error(err).await?;
            return Ok(());
        }

        let end: u64 = match message.get(NEW_SEQ_NO) {
            Ok(new_seq_no) => new_seq_no,
            Err(err) => {
                error!(
                    "received sequence reset message without new sequence number: {:?}",
                    err
                );
                let reject = Reject::new(msg_seq_num)
                    .session_reject_reason(SessionRejectReason::RequiredTagMissing)
                    .text("missing NewSeqNo tag in sequence reset message");
                self.send_message(reject)
                    .await
                    .with_send_context("reject for missing NEW_SEQ_NO")?;

                // note: we don't increment the target seq number here
                // this is an ambiguous case in the specification, but leaving the
                // sequence number as is feels the safest
                return Ok(());
            }
        };

        // sequence resets cannot move the target seq number backwards
        // regardless of whether the message is a gap fill or not
        if end <= self.ctx.store.next_target_seq_number() {
            error!(
                "received sequence reset message which would move target seq number backwards: {end}",
            );
            let text =
                format!("attempt to lower sequence number, invalid value NewSeqNo(36)={end}");
            let reject = Reject::new(msg_seq_num)
                .session_reject_reason(SessionRejectReason::ValueIsIncorrect)
                .text(&text);
            self.send_message(reject)
                .await
                .with_send_context("reject for invalid sequence reset")?;
            return Ok(());
        }

        self.ctx.store.set_target_seq_number(end - 1).await?;
        Ok(())
    }

    async fn handle_verification_error(
        &mut self,
        error: MessageVerificationError,
    ) -> Result<(), SessionOperationError> {
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
                self.handle_sequence_number_too_high(expected, actual)
                    .await?;
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

        Ok(())
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
        if let Err(err) = self.send_message(reject).await {
            error!("failed to send reject message with invalid comp ID: {err}");
        };

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
        self.state = SessionState::new_disconnected(false, &reason);
    }

    async fn handle_sequence_number_too_high(
        &mut self,
        expected: u64,
        actual: u64,
    ) -> Result<(), SessionOperationError> {
        match self
            .state
            .try_transition_to_awaiting_resend(expected, actual)
        {
            AwaitingResendTransitionOutcome::Success => {
                debug!(
                    "we are behind target (ours: {expected}, theirs: {actual}), requesting resend."
                );
                self.send_resend_request(expected, actual).await?;
            }
            AwaitingResendTransitionOutcome::InvalidState(reason) => {
                error!("failed to request resend: {reason}");
            }
            AwaitingResendTransitionOutcome::BeginSeqNumberTooLow => {
                self.state.disconnect_writer().await;
                self.state = SessionState::new_disconnected(
                    false,
                    "awaiting resend begin seq number unexpectedly lower than the previous resend request's",
                );
            }
            AwaitingResendTransitionOutcome::AttemptsExceeded => {
                self.state.disconnect_writer().await;
                self.state = SessionState::new_disconnected(
                    false,
                    "resend request attempts exceeded, manual intervention required",
                );
            }
        }

        Ok(())
    }

    async fn handle_invalid_msg_type(&mut self, message: Message, msg_type: &str) {
        match message.header().get(MSG_SEQ_NUM) {
            Ok(msg_seq_num) => {
                let reject = Reject::new(msg_seq_num)
                    .session_reject_reason(SessionRejectReason::InvalidMsgtype)
                    .text(&format!("invalid message type {msg_type}"));
                if let Err(err) = self.send_message(reject).await {
                    error!("failed to send reject message for invalid msgtype: {err}");
                };

                #[allow(clippy::collapsible_if)]
                if let Ok(seq_num) = message.header().get::<u64>(MSG_SEQ_NUM)
                    && self.ctx.store.next_target_seq_number() == seq_num
                {
                    if let Err(err) = self.ctx.store.increment_target_seq_number().await {
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
        if let Err(err) = self.send_message(reject).await {
            error!("failed to send reject for time accuracy problem: {err}");
        };
        if let Err(err) = self.ctx.store.increment_target_seq_number().await {
            error!("failed to increment target seq number: {:?}", err);
        };
    }

    async fn handle_original_sending_time_missing(&mut self, msg_seq_num: u64) {
        let reject = Reject::new(msg_seq_num)
            .session_reject_reason(SessionRejectReason::RequiredTagMissing)
            .text("original sending time is required");
        if let Err(err) = self.send_message(reject).await {
            error!("failed to send reject for time missing tag: {err}");
        };
        if let Err(err) = self.ctx.store.increment_target_seq_number().await {
            error!("failed to increment target seq number: {:?}", err);
        };
    }

    fn reset_heartbeat_timer(&mut self) {
        self.state
            .reset_heartbeat_timer(self.ctx.config.heartbeat_interval);
    }

    fn reset_peer_timer(&mut self, test_request_id: Option<TestRequestId>) {
        self.state
            .reset_peer_timer(self.ctx.config.heartbeat_interval, test_request_id);
    }

    async fn send_app_message(&mut self, message: App::Outbound) -> Result<SendOutcome, SendError> {
        if !self.state.is_connected() {
            return Err(SendError::Disconnected);
        }

        match self.ctx.application.on_outbound_message(&message).await {
            OutboundDecision::Send => {
                let sequence_number = self.send_message(message).await?;
                Ok(SendOutcome::Sent { sequence_number })
            }
            OutboundDecision::Drop => {
                debug!("dropped outbound message as instructed by the application");
                Ok(SendOutcome::Dropped)
            }
            OutboundDecision::TerminateSession => {
                warn!("the application indicated we should terminate the session");
                self.state.disconnect_writer().await;
                Err(SendError::SessionTerminated)
            }
        }
    }

    async fn send_message(
        &mut self,
        message: impl OutboundMessage,
    ) -> Result<u64, InternalSendError> {
        let msg_type = message.message_type().to_string();
        let prepared = self.ctx.prepare_message(message).await?;
        self.state.send_message(&msg_type, prepared.raw).await;
        self.reset_heartbeat_timer();
        Ok(prepared.seq_num)
    }

    async fn send_resend_request(
        &mut self,
        begin: u64,
        end: u64,
    ) -> Result<(), SessionOperationError> {
        let request = ResendRequest::new(begin, end);
        self.send_message(request)
            .await
            .with_send_context("resend request")?;
        Ok(())
    }

    async fn send_logon(&mut self) -> Result<(), SessionOperationError> {
        let reset_config = if self.ctx.config.reset_on_logon || self.reset_on_next_logon {
            self.ctx.store.reset().await?;
            ResetSeqNumConfig::Reset
        } else {
            ResetSeqNumConfig::NoReset(Some(self.ctx.store.next_target_seq_number()))
        };
        self.reset_on_next_logon = false;

        let logon = Logon::new(self.ctx.config.heartbeat_interval, reset_config);
        self.send_message(logon).await.with_send_context("logon")?;
        Ok(())
    }

    async fn send_logout(&mut self, reason: &str) -> Result<(), SessionOperationError> {
        let logout = Logout::with_reason(reason.to_string());
        self.send_message(logout)
            .await
            .with_send_context("logout")?;
        Ok(())
    }

    /// Sends a logout message and immediately disconnects the counterparty.
    ///
    /// This should be used sparingly in scenarios where there is a major issue
    /// requiring operational intervention, such as the sequence number being lower
    /// than expected, or some other key header field containing an invalid value.
    ///
    /// In other scenarios, [`initiate_graceful_logout`] should be preferred.
    async fn logout_and_terminate(&mut self, reason: &str) {
        if let Err(err) = self.send_logout(reason).await {
            warn!("failed to send logout during session termination: {}", err);
        }
        self.state.disconnect_writer().await;
    }

    /// Sends a logout message and puts the session state into an [`AwaitingLogout`] state.
    ///
    /// The session waits for a configurable timeout period for the counterparty to
    /// respond with a `Logout` message. If no response is received within the timeout
    /// period, it disconnects the counterparty.
    async fn initiate_graceful_logout(
        &mut self,
        reason: &str,
        reconnect: bool,
    ) -> Result<(), SessionOperationError> {
        if self.state.try_transition_to_awaiting_logout(
            Duration::from_secs(self.ctx.config.logout_timeout),
            reconnect,
        ) {
            self.send_logout(reason).await?;
        }

        Ok(())
    }

    async fn handle_session_event(&mut self, event: SessionEvent) {
        self.handle_schedule_check().await;

        match event {
            SessionEvent::FixMessageReceived(fix_message) => {
                if let Err(err) = self.on_incoming(fix_message).await {
                    let reason = err.to_string();
                    error!(reason, "fatal error in message processing");
                    self.logout_and_terminate("internal error").await;
                    self.state = SessionState::new_disconnected(true, &reason);
                }
            }
            SessionEvent::Disconnected(reason) => {
                warn!(reason, "disconnected from peer");
                self.on_disconnect(reason).await;
            }
            SessionEvent::Connected(w) => {
                if let Err(err) = self.on_connect(w).await {
                    error!(err = ?err, "failed to establish logon after connecting");
                }
            }
            SessionEvent::ShouldReconnect(responder) => {
                if responder.send(self.state.should_reconnect()).is_err() {
                    warn!("tried to respond to ShouldReconnect query but the receiver is gone");
                }
            }
            SessionEvent::AwaitSchedule(responder) => {
                self.state.register_schedule_awaiter(responder);
            }
        }
    }

    async fn handle_outbound_message(&mut self, request: OutboundRequest<App::Outbound>) {
        let OutboundRequest { message, confirm } = request;
        let result = self.send_app_message(message).await;
        match confirm {
            Some(tx) => {
                // Ignore send errors - receiver may have been dropped
                let _ = tx.send(result);
            }
            None => {
                if let Err(err) = result {
                    error!(err = ?err, "failed to send app message: {err}");
                }
            }
        }
    }

    async fn handle_admin_request(&mut self, request: AdminRequest) {
        match request {
            AdminRequest::InitiateGracefulShutdown { reconnect } => {
                warn!("initiating shutdown on request from admin..");
                if let Err(err) = self
                    .initiate_graceful_logout("explicitly requested", reconnect)
                    .await
                {
                    error!(err = ?err, "initiating graceful shutdown");
                }
            }
            AdminRequest::RequestSessionInfo(responder) => {
                info!("session info requested");
                if responder.send(self.get_session_info()).is_err() {
                    error!("failed to respond to session info request");
                }
            }
            AdminRequest::ResetSequenceNumbersOnNextLogon => {
                warn!("resetting sequence numbers on next logon");
                self.reset_on_next_logon = true;
            }
        }
    }

    async fn handle_heartbeat_timeout(&mut self) {
        if let Err(err) = self.send_message(Heartbeat::default()).await {
            error!(err = ?err, "failed to send heartbeat message");
        }
    }

    async fn handle_peer_timeout(&mut self) {
        if self.state.is_expecting_test_response() {
            warn!("peer didn't respond, terminating..");
            self.logout_and_terminate("peer timeout").await;
        } else if self.state.is_awaiting_logon() {
            warn!("peer didn't respond to our Logon, disconnecting..");
            self.state.disconnect_writer().await;
        } else if self.state.is_awaiting_logout() {
            warn!("peer didn't respond to our Logout, disconnecting..");
            self.state.disconnect_writer().await;
        } else {
            let req_id = format!("TEST_{}", self.ctx.store.next_target_seq_number());
            info!("sending TestRequest due to peer timer expiring");
            let request = TestRequest::new(req_id.clone());
            if let Err(err) = self.send_message(request).await {
                error!(err = ?err, "failed to send TestRequest");
            }
            self.reset_peer_timer(Some(req_id));
        }
    }

    async fn handle_schedule_check(&mut self) {
        let now = Utc::now();
        let is_active = self.schedule.is_active_at(&now);

        if is_active {
            self.state.notify_schedule_awaiter();
            match self
                .schedule
                .is_same_session_period(&self.ctx.store.creation_time(), &now)
            {
                Ok(SessionPeriodComparison::SamePeriod) => {
                    // we are in the same period, nothing needs to be done
                }
                Ok(SessionPeriodComparison::DifferentPeriod) => {
                    // the message store is for a previous session,
                    // we need to terminate this session, reset the store, and reestablish the session
                    self.logout_and_terminate("session period changed").await;
                    if let Err(err) = self.ctx.store.reset().await {
                        error!("error resetting session store: {err:}");
                        self.state =
                            SessionState::new_disconnected(false, "unexpected error in reset");
                    }
                }
                Ok(SessionPeriodComparison::OutsideSessionTime { .. }) => {
                    // the creation_time was recorded outside the session schedule,
                    // treat this similarly to a different period - reset the store
                    warn!("store creation time is outside session schedule, resetting store");
                    self.logout_and_terminate("creation time outside schedule")
                        .await;
                    if let Err(err) = self.ctx.store.reset().await {
                        error!("error resetting session store: {err:}");
                        self.state =
                            SessionState::new_disconnected(false, "unexpected error in reset");
                    }
                }
                Err(err) => {
                    // actual schedule calculation error (e.g., DST transition, date overflow)
                    error!("error checking session period: {err:?}");
                    self.logout_and_terminate("internal error").await;
                }
            }
        } else if self.state.is_connected() {
            // we are currently outside scheduled session time
            if let Err(err) = self
                .initiate_graceful_logout("End of session time", true)
                .await
            {
                error!(err = ?err, "failed to initiate graceful logout");
            }
        }

        // we always need to reschedule the check, otherwise we won't be able to resume an inactive session
        let deadline = Instant::now() + Duration::from_secs(SCHEDULE_CHECK_INTERVAL);
        self.schedule_check_timer.as_mut().reset(deadline);
    }

    fn get_session_info(&self) -> SessionInfo {
        SessionInfo {
            next_sender_seq_number: self.ctx.store.next_sender_seq_number(),
            next_target_seq_number: self.ctx.store.next_target_seq_number(),
            status: self.state.as_status(),
        }
    }
}

/// Extracts MsgSeqNum from a message header.
///
/// To be removed once https://github.com/Validus-Risk-Management/hotfix/issues/301
/// is implemented.
///
/// # Panics
/// Panics if the message does not contain a valid MsgSeqNum field.
/// This should never happen for messages that have passed validation.
#[allow(clippy::expect_used)]
fn get_msg_seq_num(message: &Message) -> u64 {
    message
        .header()
        .get(MSG_SEQ_NUM)
        .expect("MsgSeqNum missing from validated message - parser bug")
}

async fn run_session<App, Store>(
    mut session: Session<App, Store>,
    mut event_receiver: mpsc::Receiver<SessionEvent>,
    mut outbound_message_receiver: mpsc::Receiver<OutboundRequest<App::Outbound>>,
    mut admin_request_receiver: mpsc::Receiver<AdminRequest>,
) where
    App: Application,
    Store: MessageStore + Send + 'static,
{
    loop {
        select! {
            biased;
            next_admin_request = admin_request_receiver.recv() => {
                match next_admin_request {
                    Some(request) => session.handle_admin_request(request).await,
                    None => break,
                }
            }
            next_event = event_receiver.recv()=> {
                match next_event {
                    Some(event) => {
                        session.handle_session_event(event).await
                    }
                    None => break,
                }
            }
            next_outbound_request = outbound_message_receiver.recv() => {
                match next_outbound_request {
                    Some(request) => session.handle_outbound_message(request).await,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{InboundDecision, OutboundDecision};
    use crate::message::OutboundMessage;
    use crate::store::{Result as StoreResult, StoreError};
    use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeDelta, Timelike};
    use chrono_tz::Tz;
    use hotfix_message::message::Message;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::mpsc;

    /// A controllable store for testing that implements MessageStore
    #[derive(Clone)]
    struct TestStore {
        creation_time: DateTime<Utc>,
        fail_reset: Arc<AtomicBool>,
        reset_called: Arc<AtomicBool>,
        sender_seq: u64,
        target_seq: u64,
    }

    impl TestStore {
        fn new(creation_time: DateTime<Utc>) -> Self {
            Self {
                creation_time,
                fail_reset: Arc::new(AtomicBool::new(false)),
                reset_called: Arc::new(AtomicBool::new(false)),
                sender_seq: 1,
                target_seq: 1,
            }
        }

        fn set_fail_reset(&self) {
            self.fail_reset.store(true, Ordering::SeqCst);
        }

        fn was_reset_called(&self) -> bool {
            self.reset_called.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl MessageStore for TestStore {
        async fn add(&mut self, _sequence_number: u64, _message: &[u8]) -> StoreResult<()> {
            Ok(())
        }

        async fn get_slice(&self, _begin: usize, _end: usize) -> StoreResult<Vec<Vec<u8>>> {
            Ok(vec![])
        }

        fn next_sender_seq_number(&self) -> u64 {
            self.sender_seq
        }

        fn next_target_seq_number(&self) -> u64 {
            self.target_seq
        }

        async fn increment_sender_seq_number(&mut self) -> StoreResult<()> {
            self.sender_seq += 1;
            Ok(())
        }

        async fn increment_target_seq_number(&mut self) -> StoreResult<()> {
            self.target_seq += 1;
            Ok(())
        }

        async fn set_target_seq_number(&mut self, seq_number: u64) -> StoreResult<()> {
            self.target_seq = seq_number;
            Ok(())
        }

        async fn reset(&mut self) -> StoreResult<()> {
            self.reset_called.store(true, Ordering::SeqCst);
            if self.fail_reset.load(Ordering::SeqCst) {
                return Err(StoreError::Reset("simulated reset failure".into()));
            }
            self.creation_time = Utc::now();
            Ok(())
        }

        fn creation_time(&self) -> DateTime<Utc> {
            self.creation_time
        }
    }

    /// Dummy message type for testing that implements required traits
    #[derive(Clone)]
    struct DummyMessage;

    impl OutboundMessage for DummyMessage {
        fn write(&self, _msg: &mut Message) {}
        fn message_type(&self) -> &str {
            "0"
        }
    }

    /// Minimal no-op application for testing
    struct NoOpApp;

    #[async_trait::async_trait]
    impl Application for NoOpApp {
        type Outbound = DummyMessage;

        async fn on_outbound_message(&self, _: &DummyMessage) -> OutboundDecision {
            OutboundDecision::Send
        }
        async fn on_inbound_message(&self, _: &Message) -> InboundDecision {
            InboundDecision::Accept
        }
        async fn on_logout(&mut self, _: &str) {}
        async fn on_logon(&mut self) {}
    }

    fn create_writer_ref() -> WriterRef {
        let (sender, _) = mpsc::channel(10);
        WriterRef::new(sender)
    }

    fn create_test_config() -> SessionConfig {
        SessionConfig {
            begin_string: "FIX.4.4".to_string(),
            sender_comp_id: "SENDER".to_string(),
            target_comp_id: "TARGET".to_string(),
            data_dictionary_path: None,
            connection_host: "localhost".to_string(),
            connection_port: 9876,
            tls_config: None,
            heartbeat_interval: 30,
            logon_timeout: 10,
            logout_timeout: 2,
            reconnect_interval: 30,
            reset_on_logon: false,
            schedule: None,
        }
    }

    fn create_test_session(
        schedule: SessionSchedule,
        state: SessionState,
        store: TestStore,
    ) -> Session<NoOpApp, TestStore> {
        let config = create_test_config();
        let message_config = MessageConfig::default();
        let dictionary = Dictionary::fix44();
        let message_builder = MessageBuilder::new(dictionary, message_config).unwrap();
        let ctx = SessionCtx {
            config,
            store,
            application: NoOpApp,
            message_builder,
            message_config,
        };

        Session {
            ctx,
            schedule,
            state,
            schedule_check_timer: Box::pin(sleep(Duration::from_secs(1))),
            reset_on_next_logon: false,
        }
    }

    /// Creates a Daily schedule that is active at the current time
    fn create_active_schedule() -> SessionSchedule {
        // Use a 24-hour window that's definitely active
        SessionSchedule::Daily {
            start_time: NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
            end_time: NaiveTime::from_hms_opt(23, 59, 59).unwrap(),
            timezone: Tz::UTC,
        }
    }

    /// Creates a Daily schedule that is inactive at the current time
    fn create_inactive_schedule() -> SessionSchedule {
        let now = Utc::now();
        let current_hour = now.time().hour();
        // Create a 1-hour window that's 12 hours from now (definitely not the current hour)
        let start_hour = (current_hour + 12) % 24;
        let end_hour = (start_hour + 1) % 24;
        SessionSchedule::Daily {
            start_time: NaiveTime::from_hms_opt(start_hour, 0, 0).unwrap(),
            end_time: NaiveTime::from_hms_opt(end_hour, 0, 0).unwrap(),
            timezone: Tz::UTC,
        }
    }

    #[tokio::test]
    async fn test_handle_schedule_check_active_same_period() {
        // Use NonStop schedule - always active, always same period
        let schedule = SessionSchedule::NonStop;
        let writer = create_writer_ref();
        let state = SessionState::new_active(writer, 30);
        let store = TestStore::new(Utc::now());

        let mut session = create_test_session(schedule, state, store);

        session.handle_schedule_check().await;

        // State should remain Active (no logout triggered)
        assert!(
            session.state.is_logged_on(),
            "State should remain logged on for same period"
        );
        assert!(
            !session.ctx.store.was_reset_called(),
            "Store reset should not be called for same period"
        );
    }

    #[tokio::test]
    async fn test_handle_schedule_check_active_different_period() {
        use crate::session_schedule::SessionPeriodComparison;

        // Use a Daily schedule that's currently active
        let schedule = create_active_schedule();
        let writer = create_writer_ref();
        let state = SessionState::new_active(writer, 30);
        // Creation time is yesterday - different session period
        let yesterday = Utc::now() - TimeDelta::days(1);
        let store = TestStore::new(yesterday);

        let mut session = create_test_session(schedule, state, store);

        // Verify the schedule correctly identifies different periods
        let now = Utc::now();
        let creation_time = session.ctx.store.creation_time();
        let same_period = session
            .schedule
            .is_same_session_period(&creation_time, &now);
        assert!(
            matches!(same_period, Ok(SessionPeriodComparison::DifferentPeriod)),
            "Schedule should identify different periods"
        );

        session.handle_schedule_check().await;

        // Store reset should have been called (indicates DifferentPeriod branch was taken)
        // Note: logout_and_terminate disconnects the writer but state transition to
        // Disconnected happens asynchronously via event processing, not in this call
        assert!(
            session.ctx.store.was_reset_called(),
            "Store reset should be called for different period"
        );
    }

    #[tokio::test]
    async fn test_handle_schedule_check_active_reset_fails() {
        // Use a Daily schedule that's currently active
        let schedule = create_active_schedule();
        let writer = create_writer_ref();
        let state = SessionState::new_active(writer, 30);
        // Creation time is yesterday - different session period
        let yesterday = Utc::now() - TimeDelta::days(1);
        let store = TestStore::new(yesterday);
        store.set_fail_reset();

        let mut session = create_test_session(schedule, state, store);

        session.handle_schedule_check().await;

        // Store reset should have been attempted
        assert!(
            session.ctx.store.was_reset_called(),
            "Store reset should be called"
        );
        // When reset fails, state is explicitly set to Disconnected(reconnect=false)
        assert!(
            matches!(session.state, SessionState::Disconnected(_)),
            "State should be Disconnected after reset failure"
        );
        // Should NOT reconnect since reset failed
        assert!(
            !session.state.should_reconnect(),
            "Should not reconnect after failed reset"
        );
    }

    #[tokio::test]
    async fn test_handle_schedule_check_active_creation_time_outside_schedule() {
        use crate::session_schedule::SessionPeriodComparison;

        // Use a narrow schedule that's currently active but creation_time is outside
        let now = Utc::now();
        let current_hour = now.time().hour();

        // Create a 2-hour window around current time
        let start_hour = if current_hour == 0 {
            23
        } else {
            current_hour - 1
        };
        let end_hour = (current_hour + 2) % 24;

        let schedule = SessionSchedule::Daily {
            start_time: NaiveTime::from_hms_opt(start_hour, 0, 0).unwrap(),
            end_time: NaiveTime::from_hms_opt(end_hour, 0, 0).unwrap(),
            timezone: Tz::UTC,
        };

        let writer = create_writer_ref();
        let state = SessionState::new_active(writer, 30);

        // Creation time is today but at a time outside the schedule window
        // Use a time that's definitely outside the window (6 hours from now)
        let outside_hour = (current_hour + 6) % 24;
        let creation_time = DateTime::from_naive_utc_and_offset(
            NaiveDate::from_ymd_opt(now.year(), now.month(), now.day())
                .unwrap()
                .and_hms_opt(outside_hour, 30, 0)
                .unwrap(),
            Utc,
        );

        let store = TestStore::new(creation_time);

        let mut session = create_test_session(schedule, state, store);

        // Verify that is_same_session_period returns OutsideSessionTime
        let same_period = session
            .schedule
            .is_same_session_period(&creation_time, &now);
        assert!(
            matches!(
                same_period,
                Ok(SessionPeriodComparison::OutsideSessionTime { .. })
            ),
            "Schedule should return OutsideSessionTime when creation_time is outside active window"
        );

        session.handle_schedule_check().await;

        // The OutsideSessionTime branch now triggers a store reset (same as DifferentPeriod)
        assert!(
            session.ctx.store.was_reset_called(),
            "Store reset should be called when creation_time is outside schedule"
        );
    }

    #[tokio::test]
    async fn test_handle_schedule_check_inactive_connected() {
        // Use a schedule that's currently inactive
        let schedule = create_inactive_schedule();
        let writer = create_writer_ref();
        let state = SessionState::new_active(writer, 30);
        let store = TestStore::new(Utc::now());

        let mut session = create_test_session(schedule, state, store);

        session.handle_schedule_check().await;

        // State should be AwaitingLogout (graceful logout initiated)
        assert!(
            session.state.is_awaiting_logout(),
            "State should be AwaitingLogout when schedule is inactive and was connected"
        );
    }

    #[tokio::test]
    async fn test_handle_schedule_check_inactive_disconnected() {
        // Use a schedule that's currently inactive
        let schedule = create_inactive_schedule();
        let state = SessionState::new_disconnected(true, "test");
        let store = TestStore::new(Utc::now());

        let mut session = create_test_session(schedule, state, store);

        session.handle_schedule_check().await;

        // State should remain Disconnected (no action taken)
        assert!(
            matches!(session.state, SessionState::Disconnected(_)),
            "State should remain Disconnected when schedule is inactive and was disconnected"
        );
        // Reconnect flag should be preserved
        assert!(
            session.state.should_reconnect(),
            "Reconnect flag should be preserved"
        );
    }
}
