pub(crate) mod admin_request;
pub mod error;
pub(crate) mod event;
mod info;
mod session_handle;
pub mod session_ref;
mod state;

use crate::config::SessionConfig;
use crate::message::OutboundMessage;
use crate::message::heartbeat::Heartbeat;
use crate::message::logon::{Logon, ResetSeqNumConfig};
use crate::message::parser::RawFixMessage;
use crate::message::{InboundMessage, generate_message};
use crate::store::MessageStore;
use crate::transport::writer::WriterRef;
use anyhow::{Context, Result, anyhow, bail};
use chrono::Utc;
use hotfix_message::dict::Dictionary;
use hotfix_message::message::{Config as MessageConfig, Message};
use hotfix_message::{MessageBuilder, Part};
use std::pin::Pin;
use tokio::select;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant, Sleep, sleep, sleep_until};
use tracing::{debug, enabled, error, info, warn};

use crate::Application;
use crate::application::{InboundDecision, OutboundDecision};
use crate::message::logout::Logout;
use crate::message::reject::Reject;
use crate::message::resend_request::ResendRequest;
use crate::message::sequence_reset::SequenceReset;
use crate::message::test_request::TestRequest;
use crate::message::verification::verify_message;
use crate::message::verification_error::{CompIdType, MessageVerificationError};
use crate::message_utils::{is_admin, prepare_message_for_resend};
use crate::session::admin_request::AdminRequest;
pub use crate::session::error::{SendError, SendOutcome};
pub use crate::session::info::{SessionInfo, Status};
pub use crate::session::session_handle::SessionHandle;
#[cfg(not(feature = "test-utils"))]
pub(crate) use crate::session::session_ref::InternalSessionRef;
#[cfg(feature = "test-utils")]
pub use crate::session::session_ref::InternalSessionRef;
use crate::session::session_ref::OutboundRequest;
use crate::session::state::SessionState;
use crate::session::state::{AwaitingResendTransitionOutcome, TestRequestId};
use crate::session_schedule::SessionSchedule;
use event::SessionEvent;
use hotfix_message::parsed_message::{InvalidReason, ParsedMessage};
use hotfix_message::session_fields::{
    BEGIN_SEQ_NO, END_SEQ_NO, GAP_FILL_FLAG, MSG_SEQ_NUM, MSG_TYPE, NEW_SEQ_NO,
    SessionRejectReason, TEST_REQ_ID,
};

const SCHEDULE_CHECK_INTERVAL: u64 = 1;

struct Session<A, I, O, S> {
    message_config: MessageConfig,
    config: SessionConfig,
    schedule: SessionSchedule,
    message_builder: MessageBuilder,
    state: SessionState,
    application: A,
    store: S,
    schedule_check_timer: Pin<Box<Sleep>>,
    reset_on_next_logon: bool,
    _phantom: std::marker::PhantomData<fn() -> (I, O)>,
}

impl<App, Inbound, Outbound, Store> Session<App, Inbound, Outbound, Store>
where
    App: Application<Inbound, Outbound>,
    Inbound: InboundMessage,
    Outbound: OutboundMessage,
    Store: MessageStore,
{
    fn new(
        config: SessionConfig,
        application: App,
        store: Store,
    ) -> Result<Session<App, Inbound, Outbound, Store>> {
        let schedule_check_timer = sleep(Duration::from_secs(SCHEDULE_CHECK_INTERVAL));

        let dictionary = Self::get_data_dictionary(&config)?;
        let message_config = MessageConfig::default();
        let message_builder = MessageBuilder::new(dictionary, message_config)?;
        let schedule = config.schedule.as_ref().try_into()?;

        let session = Self {
            config,
            schedule,
            message_config,
            message_builder,
            state: SessionState::new_disconnected(true, "initialising"),
            application,
            store,
            schedule_check_timer: Box::pin(schedule_check_timer),
            reset_on_next_logon: false,
            _phantom: std::marker::PhantomData,
        };

        Ok(session)
    }

    fn get_data_dictionary(config: &SessionConfig) -> Result<Dictionary> {
        match &config.data_dictionary_path {
            None => match config.begin_string.as_str() {
                #[cfg(feature = "fix44")]
                "FIX.4.4" => Ok(Dictionary::fix44()),
                _ => bail!("unsupported begin string: {}", config.begin_string),
            },
            Some(dictionary_path) => Ok(Dictionary::load_from_file(dictionary_path)?),
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
                    match message.header().get(MSG_SEQ_NUM) {
                        Ok(msg_seq_num) => {
                            let reject = Reject::new(msg_seq_num)
                                .session_reject_reason(SessionRejectReason::InvalidTagNumber)
                                .text(&format!("invalid field {tag}"));
                            self.send_message(reject)
                                .await
                                .context("failed to send reject")?;
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
                                .context("failed to send reject")?;
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
        let message_type = message.header().get(MSG_TYPE)?;

        if let SessionState::AwaitingResend(state) = &mut self.state {
            let seq_number: u64 = message
                .header()
                .get(MSG_SEQ_NUM)
                .map_err(|e| anyhow!("failed to get seq number: {:?}", e))?;
            if seq_number > state.end_seq_number {
                state.inbound_queue.push_back(message);
                return Ok(());
            }
        }

        if let SessionState::AwaitingLogon { .. } = &mut self.state {
            // TODO: should this (and all inbound message processing) logic be pushed into the state?
            if message_type != "A" {
                self.state.disconnect_writer().await;
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
        match self.verify_message(message, true) {
            Ok(_) => {
                let parsed_message = Inbound::parse(message);
                if matches!(
                    self.application.on_inbound_message(parsed_message).await,
                    InboundDecision::TerminateSession
                ) {
                    error!("failed to send inbound message to application");
                    self.state.disconnect_writer().await;
                }
                self.store.increment_target_seq_number().await?;
            }
            Err(err) => self
                .handle_verification_error(err)
                .await
                .context("failed to handle verification error")?,
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
                let seq_number: u64 = msg.get(MSG_SEQ_NUM).unwrap_or_else(|e| {
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

    fn verify_message(
        &self,
        message: &Message,
        verify_target_seq_number: bool,
    ) -> std::result::Result<(), MessageVerificationError> {
        let expected_seq_number = if verify_target_seq_number {
            Some(self.store.next_target_seq_number())
        } else {
            None
        };
        verify_message(message, &self.config, expected_seq_number)
    }

    async fn on_connect(&mut self, writer: WriterRef) -> Result<()> {
        self.state = SessionState::AwaitingLogon {
            writer,
            logon_sent: false,
            logon_timeout: Instant::now() + Duration::from_secs(self.config.logon_timeout),
        };
        self.reset_peer_timer(None);
        self.send_logon().await?;

        Ok(())
    }

    async fn on_disconnect(&mut self, reason: String) {
        match self.state {
            SessionState::Active { .. }
            | SessionState::AwaitingLogon { .. }
            | SessionState::AwaitingResend(_) => {
                self.state.disconnect_writer().await;
                self.state = SessionState::new_disconnected(true, &reason);
            }
            SessionState::Disconnected { .. } => {
                warn!("disconnect message was received, but the session is already disconnected")
            }
            SessionState::AwaitingLogout { reconnect, .. } => {
                self.state = SessionState::new_disconnected(reconnect, &reason);
            }
        }
    }

    async fn on_logon(&mut self, message: &Message) -> Result<()> {
        if let SessionState::AwaitingLogon { writer, .. } = &self.state {
            match self.verify_message(message, true) {
                Ok(_) => {
                    // happy logon flow, the session is now active
                    self.state =
                        SessionState::new_active(writer.clone(), self.config.heartbeat_interval);
                    self.application.on_logon().await;
                    self.store.increment_target_seq_number().await?;
                }
                Err(err) => self
                    .handle_verification_error(err)
                    .await
                    .context("failed to handle verification error")?,
            }
        } else {
            error!("received unexpected logon message");
        }

        Ok(())
    }

    async fn on_logout(&mut self) -> Result<()> {
        if self.state.is_logged_on() {
            self.send_logout("Logout acknowledged").await?;
        }

        self.application.on_logout("peer has logged us out").await;

        match self.state {
            // if the session is already disconnected, we have nothing else to do
            SessionState::Disconnected(..) => {}
            // otherwise set the state to disconnected and assume it makes sense to try to reconnect
            _ => {
                self.state.disconnect_writer().await;
                self.state = SessionState::new_disconnected(true, "peer has logged us out")
            }
        }

        self.store.increment_target_seq_number().await?;
        Ok(())
    }

    async fn on_heartbeat(&mut self, message: &Message) -> Result<()> {
        if let (Some(expected_req_id), Ok(message_req_id)) = (
            &self.state.expected_test_response_id(),
            message.get::<&str>(TEST_REQ_ID),
        ) && expected_req_id.as_str() == message_req_id
        {
            debug!("received response for TestRequest, resetting timer");
            self.reset_peer_timer(None);
        }

        self.store.increment_target_seq_number().await?;
        Ok(())
    }

    async fn on_test_request(&mut self, message: &Message) -> Result<()> {
        let req_id: &str = message.get(TEST_REQ_ID).unwrap_or_else(|_| {
            // TODO: send reject?
            todo!()
        });

        self.store.increment_target_seq_number().await?;

        self.send_message(Heartbeat::for_request(req_id.to_string()))
            .await
            .context("failed to send heartbeat in response to test request")?;

        Ok(())
    }

    async fn on_resend_request(&mut self, message: &Message) -> Result<()> {
        if !self.state.is_connected() {
            warn!("received resend request while disconnected, ignoring");
        }

        let begin_seq_number: u64 = match message.get(BEGIN_SEQ_NO) {
            Ok(seq_number) => seq_number,
            Err(_) => {
                let reject = Reject::new(
                    message
                        .header()
                        .get(MSG_SEQ_NUM)
                        .map_err(|_| anyhow!("failed to get seq number"))?,
                )
                .session_reject_reason(SessionRejectReason::RequiredTagMissing)
                .text("missing begin sequence number for resend request");
                self.send_message(reject)
                    .await
                    .context("failed to send reject for invalid resend request")?;
                return Ok(());
            }
        };

        let end_seq_number: u64 = match message.get(END_SEQ_NO) {
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
                        .get(MSG_SEQ_NUM)
                        .map_err(|_| anyhow!("failed to get seq number"))?,
                )
                .session_reject_reason(SessionRejectReason::RequiredTagMissing)
                .text("missing end sequence number for resend request");
                self.send_message(reject)
                    .await
                    .context("failed to send reject for invalid resend request")?;
                return Ok(());
            }
        };

        self.store.increment_target_seq_number().await?;

        self.resend_messages(begin_seq_number, end_seq_number, message)
            .await?;

        Ok(())
    }

    /// Handle Reject messages.
    ///
    /// Returns whether the message should be processed as usual
    /// and whether the target sequence number should be incremented.
    async fn on_reject(&mut self, message: &Message) -> Result<()> {
        if let Ok(seq_num) = message.get::<u64>(MSG_SEQ_NUM)
            && seq_num == self.store.next_target_seq_number()
        {
            self.store.increment_target_seq_number().await?;
        }

        Ok(())
    }

    async fn on_sequence_reset(&mut self, message: &Message) -> Result<()> {
        let msg_seq_num = message
            .header()
            .get(MSG_SEQ_NUM)
            .map_err(|_| anyhow!("failed to get seq number"))?;
        let is_gap_fill: bool = message.get(GAP_FILL_FLAG).unwrap_or(false);
        if let Err(err) = self.verify_message(message, is_gap_fill) {
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
                self.send_message(reject).await.context(
                    "failed to send reject message in response to invalid sequence reset message",
                )?;

                // note: we don't increment the target seq number here
                // this is an ambiguous case in the specification, but leaving the
                // sequence number as is feels the safest
                return Ok(());
            }
        };

        // sequence resets cannot move the target seq number backwards
        // regardless of whether the message is a gap fill or not
        if end <= self.store.next_target_seq_number() {
            error!(
                "received sequence reset message which would move target seq number backwards: {end}",
            );
            let text =
                format!("attempt to lower sequence number, invalid value NewSeqNo(36)={end}");
            let reject = Reject::new(msg_seq_num)
                .session_reject_reason(SessionRejectReason::ValueIsIncorrect)
                .text(&text);
            self.send_message(reject).await.context(
                "failed to send reject message in response to invalid sequence reset message",
            )?;
            return Ok(());
        }

        self.store.set_target_seq_number(end - 1).await?;
        Ok(())
    }

    async fn handle_verification_error(&mut self, error: MessageVerificationError) -> Result<()> {
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

    async fn handle_sequence_number_too_high(&mut self, expected: u64, actual: u64) -> Result<()> {
        match self
            .state
            .try_transition_to_awaiting_resend(expected, actual)
        {
            AwaitingResendTransitionOutcome::Success => {
                debug!(
                    "we are behind target (ours: {expected}, theirs: {actual}), requesting resend."
                );
                self.send_resend_request(expected, actual)
                    .await
                    .context("failed to send resend request")?;
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
        if let Err(err) = self.send_message(reject).await {
            error!("failed to send reject for time accuracy problem: {err}");
        };
        if let Err(err) = self.store.increment_target_seq_number().await {
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
        if let Err(err) = self.store.increment_target_seq_number().await {
            error!("failed to increment target seq number: {:?}", err);
        };
    }

    async fn resend_messages(&mut self, begin: u64, end: u64, _message: &Message) -> Result<()> {
        info!(begin, end, "resending messages as requested");
        let messages = self
            .store
            .get_slice(begin as usize, end as usize)
            .await
            .context("failed to retrieve messages from store")?;

        let no = messages.len();
        debug!(number_of_messages = no, "number of messages");

        let mut reset_start: Option<u64> = None;
        let mut sequence_number = 0;

        for msg in messages {
            let mut message = self
                .message_builder
                .build(msg.as_slice())
                .into_message()
                .with_context(|| format!("failed to build message for raw message: {msg:?}"))?;
            sequence_number = message.header().get::<u64>(MSG_SEQ_NUM).map_err(|e| {
                anyhow!(
                    "sequence number in message to resend is unexpectedly missing: {:?}",
                    e
                )
            })?;
            let message_type: String = message
                .header()
                .get::<&str>(MSG_TYPE)
                .context("message type in message to resend is unexpectedly missing")?
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
                self.send_sequence_reset(begin, end)
                    .await
                    .context("failed to send sequence reset")?;
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
                message
                    .encode(&self.message_config)
                    .context("failed to encode message")?,
            )
            .await;

            if enabled!(tracing::Level::DEBUG)
                && let Ok(m) = String::from_utf8(msg.clone())
            {
                debug!(sequence_number, message = m, "resent message");
            }
        }

        if let Some(begin) = reset_start {
            // the final reset if needed
            let end = sequence_number;
            Self::log_skipped_admin_messages(begin, end);
            self.send_sequence_reset(begin, end)
                .await
                .context("failed to send sequence reset")?;
        }

        Ok(())
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

    async fn send_app_message(&mut self, message: Outbound) -> Result<SendOutcome, SendError> {
        if !self.state.is_connected() {
            return Err(SendError::Disconnected);
        }

        match self.application.on_outbound_message(&message).await {
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

    async fn send_message(&mut self, message: impl OutboundMessage) -> Result<u64, SendError> {
        let seq_num = self.store.next_sender_seq_number();
        let msg_type = message.message_type().as_bytes().to_vec();
        let msg = generate_message(
            &self.config.begin_string,
            &self.config.sender_comp_id,
            &self.config.target_comp_id,
            seq_num,
            message,
        )
        .map_err(|e| {
            SendError::Persist(crate::store::StoreError::PersistMessage {
                sequence_number: seq_num,
                source: e.into(),
            })
        })?;

        self.store
            .increment_sender_seq_number()
            .await
            .map_err(SendError::SequenceNumber)?;

        self.store
            .add(seq_num, &msg)
            .await
            .map_err(SendError::Persist)?;

        self.send_raw(&msg_type, msg).await;

        Ok(seq_num)
    }

    async fn send_raw(&mut self, message_type: &[u8], data: Vec<u8>) {
        self.state
            .send_message(message_type, RawFixMessage::new(data))
            .await;
        self.reset_heartbeat_timer();
    }

    async fn send_sequence_reset(&mut self, begin: u64, end: u64) -> Result<()> {
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
        .context("failed to generate message")?;

        self.send_raw(b"4", raw_message).await;
        debug!(begin, end, "sent reset sequence");

        Ok(())
    }

    async fn send_resend_request(&mut self, begin: u64, end: u64) -> Result<()> {
        let request = ResendRequest::new(begin, end);
        self.send_message(request).await.map(|_| ())?;
        Ok(())
    }

    async fn send_logon(&mut self) -> Result<()> {
        let reset_config = if self.config.reset_on_logon || self.reset_on_next_logon {
            self.store.reset().await?;
            ResetSeqNumConfig::Reset
        } else {
            ResetSeqNumConfig::NoReset(Some(self.store.next_target_seq_number()))
        };
        self.reset_on_next_logon = false;

        let logon = Logon::new(self.config.heartbeat_interval, reset_config);

        self.send_message(logon).await.map(|_| ())?;
        Ok(())
    }

    async fn send_logout(&mut self, reason: &str) -> Result<()> {
        let logout = Logout::with_reason(reason.to_string());
        self.send_message(logout).await.map(|_| ())?;
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
    async fn initiate_graceful_logout(&mut self, reason: &str, reconnect: bool) -> Result<()> {
        if self.state.try_transition_to_awaiting_logout(
            Duration::from_secs(self.config.logout_timeout),
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
            SessionEvent::AwaitingActiveSession(responder) => {
                self.state.register_session_awaiter(responder);
            }
        }
    }

    async fn handle_outbound_message(&mut self, request: OutboundRequest<Outbound>) {
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
            let req_id = format!("TEST_{}", self.store.next_target_seq_number());
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
            next_sender_seq_number: self.store.next_sender_seq_number(),
            next_target_seq_number: self.store.next_target_seq_number(),
            status: self.state.as_status(),
        }
    }
}

async fn run_session<App, Inbound, Outbound, Store>(
    mut session: Session<App, Inbound, Outbound, Store>,
    mut event_receiver: mpsc::Receiver<SessionEvent>,
    mut outbound_message_receiver: mpsc::Receiver<OutboundRequest<Outbound>>,
    mut admin_request_receiver: mpsc::Receiver<AdminRequest>,
) where
    App: Application<Inbound, Outbound>,
    Inbound: InboundMessage,
    Outbound: OutboundMessage,
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
    use crate::message::{InboundMessage, OutboundMessage};
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

    impl InboundMessage for DummyMessage {
        fn parse(_message: &Message) -> Self {
            DummyMessage
        }
    }

    /// Minimal no-op application for testing
    struct NoOpApp;

    #[async_trait::async_trait]
    impl Application<DummyMessage, DummyMessage> for NoOpApp {
        async fn on_outbound_message(&self, _: &DummyMessage) -> OutboundDecision {
            OutboundDecision::Send
        }
        async fn on_inbound_message(&self, _: DummyMessage) -> InboundDecision {
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
    ) -> Session<NoOpApp, DummyMessage, DummyMessage, TestStore> {
        let config = create_test_config();
        let message_config = MessageConfig::default();
        let dictionary = Dictionary::fix44();
        let message_builder = MessageBuilder::new(dictionary, message_config).unwrap();

        Session {
            message_config,
            config,
            schedule,
            message_builder,
            state,
            application: NoOpApp,
            store,
            schedule_check_timer: Box::pin(sleep(Duration::from_secs(1))),
            reset_on_next_logon: false,
            _phantom: std::marker::PhantomData,
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
            !session.store.was_reset_called(),
            "Store reset should not be called for same period"
        );
    }

    #[tokio::test]
    async fn test_handle_schedule_check_active_different_period() {
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
        let creation_time = session.store.creation_time();
        let same_period = session
            .schedule
            .is_same_session_period(&creation_time, &now);
        assert!(
            matches!(same_period, Ok(false)),
            "Schedule should identify different periods"
        );

        session.handle_schedule_check().await;

        // Store reset should have been called (indicates Ok(false) branch was taken)
        // Note: logout_and_terminate disconnects the writer but state transition to
        // Disconnected happens asynchronously via event processing, not in this call
        assert!(
            session.store.was_reset_called(),
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
            session.store.was_reset_called(),
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
    async fn test_handle_schedule_check_active_period_error() {
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

        // Verify that is_same_session_period will return an error
        let same_period = session
            .schedule
            .is_same_session_period(&creation_time, &now);
        assert!(
            same_period.is_err(),
            "Schedule should return error when creation_time is outside active window"
        );

        session.handle_schedule_check().await;

        // The Err branch calls logout_and_terminate which disconnects the writer.
        // Store reset is NOT called in the Err branch, only in Ok(false).
        assert!(
            !session.store.was_reset_called(),
            "Store reset should not be called on period check error"
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
