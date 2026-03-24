pub(crate) mod admin_request;
mod ctx;
pub mod error;
pub(crate) mod event;
mod inbound;
mod info;
mod outbound;
mod session_handle;
pub mod session_ref;
mod state;
#[cfg(test)]
mod test_utils;

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
use crate::message::verification::VerificationFlags;
use crate::session::admin_request::AdminRequest;
use crate::session::ctx::{SessionCtx, TransitionResult, VerificationResult};
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
use crate::session::state::{AwaitingLogonState, AwaitingLogoutState, TestRequestId};
use crate::session_schedule::{SessionPeriodComparison, SessionSchedule};
use crate::store::MessageStore;
use crate::transport::writer::WriterRef;
use event::SessionEvent;
use hotfix_message::parsed_message::{InvalidReason, ParsedMessage};
use hotfix_message::session_fields::{MSG_SEQ_NUM, MSG_TYPE, SessionRejectReason, TEST_REQ_ID};

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
                    if let Some(writer) = self.state.get_writer() {
                        inbound::handle_invalid_msg_type(
                            &mut self.ctx,
                            writer,
                            &message,
                            &msg_type,
                        )
                        .await;
                    }
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

        // TODO: add state-level pre-process check that validates whether the message type
        // is acceptable in the current state (e.g. AwaitingLogon rejects non-Logon,
        // unexpected Logon in Active should be rejected per FIX spec).
        if let SessionState::AwaitingLogon(_) = &mut self.state
            && message_type != Logon::MSG_TYPE
        {
            self.state.disconnect_writer().await;
            return Ok(());
        }

        let flags = VerificationFlags::for_message(&message)?;
        if let VerificationResult::Issue(result) = self
            .state
            .handle_verification_issue(&mut self.ctx, &message, flags)
            .await?
        {
            self.apply_transition(result).await;
            return Ok(());
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
                self.on_reject().await?;
            }
            SequenceReset::MSG_TYPE => {
                self.on_sequence_reset(&message).await?;
            }
            Logout::MSG_TYPE => {
                self.on_logout().await?;
            }
            Logon::MSG_TYPE => {
                self.on_logon().await?;
            }
            _ => self.process_app_message(&message).await?,
        }

        Ok(())
    }

    async fn process_app_message(
        &mut self,
        message: &Message,
    ) -> Result<(), SessionOperationError> {
        match self.ctx.application.on_inbound_message(message).await {
            InboundDecision::Accept => {}
            InboundDecision::Reject { reason, text } => {
                let msg_type: &str = message
                    .header()
                    .get(MSG_TYPE)
                    .map_err(|_| SessionOperationError::MissingField("MSG_TYPE"))?;
                let mut reject =
                    BusinessReject::new(msg_type, reason).ref_seq_num(get_msg_seq_num(message));
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

        Ok(())
    }

    async fn check_end_of_resend(&mut self) -> Result<(), SessionOperationError> {
        let backlog = if let SessionState::AwaitingResend(state) = &mut self.state {
            if self.ctx.store.next_target_seq_number() > state.end_seq_number {
                let inbound_queue = std::mem::take(&mut state.inbound_queue);
                let new_state = SessionState::new_active(
                    state.writer.clone(),
                    self.ctx.config.heartbeat_interval,
                );
                self.apply_transition(TransitionResult::TransitionTo(new_state))
                    .await;
                Some(inbound_queue)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(mut inbound_queue) = backlog {
            // we have reached the end of the resend,
            // process queued messages and resume normal operation
            debug!("resend is done, processing backlog");
            while let Some(msg) = inbound_queue.pop_front() {
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

    async fn on_connect(&mut self, writer: WriterRef) -> Result<(), SessionOperationError> {
        self.apply_transition(TransitionResult::TransitionTo(SessionState::AwaitingLogon(
            AwaitingLogonState {
                writer,
                logon_sent: false,
                logon_timeout: Instant::now() + Duration::from_secs(self.ctx.config.logon_timeout),
            },
        )))
        .await;
        self.reset_peer_timer(None);
        self.send_logon().await?;

        Ok(())
    }

    async fn on_disconnect(&mut self, reason: String) {
        let transition = match self.state {
            SessionState::Active(_)
            | SessionState::AwaitingLogon(_)
            | SessionState::AwaitingResend(_) => {
                self.state.disconnect_writer().await;
                TransitionResult::TransitionTo(SessionState::new_disconnected(true, &reason))
            }
            SessionState::Disconnected(_) => {
                warn!("disconnect message was received, but the session is already disconnected");
                TransitionResult::Stay
            }
            SessionState::AwaitingLogout(AwaitingLogoutState { reconnect, .. }) => {
                TransitionResult::TransitionTo(SessionState::new_disconnected(reconnect, &reason))
            }
        };
        self.apply_transition(transition).await;
    }

    async fn on_logon(&mut self) -> Result<(), SessionOperationError> {
        if let SessionState::AwaitingLogon(AwaitingLogonState { writer, .. }) = &self.state {
            let writer = writer.clone();
            // happy logon flow, the session is now active
            self.apply_transition(TransitionResult::TransitionTo(SessionState::new_active(
                writer,
                self.ctx.config.heartbeat_interval,
            )))
            .await;
            self.ctx.application.on_logon().await;
            self.ctx.store.increment_target_seq_number().await?;
        } else {
            error!("received unexpected logon message");
        }

        Ok(())
    }

    async fn on_logout(&mut self) -> Result<(), SessionOperationError> {
        if self.state.is_logged_on() {
            self.state
                .send_logout(&mut self.ctx, "Logout acknowledged")
                .await?;
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
                self.apply_transition(TransitionResult::TransitionTo(
                    SessionState::new_disconnected(reconnect, "logout completed"),
                ))
                .await;
            }
            // otherwise assume it makes sense to try to reconnect
            _ => {
                self.state.disconnect_writer().await;
                self.apply_transition(TransitionResult::TransitionTo(
                    SessionState::new_disconnected(true, "peer has logged us out"),
                ))
                .await;
            }
        }

        self.ctx.store.increment_target_seq_number().await?;
        Ok(())
    }

    async fn on_heartbeat(&mut self, message: &Message) -> Result<(), SessionOperationError> {
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
        if let Some(writer) = self.state.get_writer() {
            inbound::on_test_request(&mut self.ctx, writer, message).await?;
            self.reset_heartbeat_timer();
        }
        Ok(())
    }

    async fn on_resend_request(&mut self, message: &Message) -> Result<(), SessionOperationError> {
        if !self.state.is_connected() {
            warn!("received resend request while disconnected, ignoring");
            return Ok(());
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

        if let Some(writer) = self.state.get_writer() {
            inbound::on_resend_request(&mut self.ctx, writer, message).await?;
            self.reset_heartbeat_timer();
        }

        Ok(())
    }

    /// Handle Reject messages.
    async fn on_reject(&mut self) -> Result<(), SessionOperationError> {
        self.ctx.store.increment_target_seq_number().await?;
        Ok(())
    }

    async fn on_sequence_reset(&mut self, message: &Message) -> Result<(), SessionOperationError> {
        if let Some(writer) = self.state.get_writer() {
            inbound::on_sequence_reset(&mut self.ctx, writer, message).await?;
            self.reset_heartbeat_timer();
        }
        Ok(())
    }

    async fn apply_transition(&mut self, result: TransitionResult) {
        if let TransitionResult::TransitionTo(new_state) = result {
            let old_status = self.state.as_status();
            self.state = new_state;
            let new_status = self.state.as_status();
            if old_status != new_status {
                self.ctx
                    .application
                    .on_state_change(&old_status, &new_status)
                    .await;
            }
        }
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
        self.state.send_message(&mut self.ctx, message).await
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

    async fn handle_session_event(&mut self, event: SessionEvent) {
        self.handle_schedule_check().await;

        match event {
            SessionEvent::FixMessageReceived(fix_message) => {
                if let Err(err) = self.on_incoming(fix_message).await {
                    let reason = err.to_string();
                    error!(reason, "fatal error in message processing");
                    self.state
                        .logout_and_terminate(&mut self.ctx, "internal error")
                        .await;
                    self.apply_transition(TransitionResult::TransitionTo(
                        SessionState::new_disconnected(true, &reason),
                    ))
                    .await;
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
                match self
                    .state
                    .initiate_graceful_logout(&mut self.ctx, "explicitly requested", reconnect)
                    .await
                {
                    Ok(result) => self.apply_transition(result).await,
                    Err(err) => error!(err = ?err, "initiating graceful shutdown"),
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
            self.state
                .logout_and_terminate(&mut self.ctx, "peer timeout")
                .await;
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
                    self.state
                        .logout_and_terminate(&mut self.ctx, "session period changed")
                        .await;
                    if let Err(err) = self.ctx.store.reset().await {
                        error!("error resetting session store: {err:}");
                        self.apply_transition(TransitionResult::TransitionTo(
                            SessionState::new_disconnected(false, "unexpected error in reset"),
                        ))
                        .await;
                    }
                }
                Ok(SessionPeriodComparison::OutsideSessionTime { .. }) => {
                    // the creation_time was recorded outside the session schedule,
                    // treat this similarly to a different period - reset the store
                    warn!("store creation time is outside session schedule, resetting store");
                    self.state
                        .logout_and_terminate(&mut self.ctx, "creation time outside schedule")
                        .await;
                    if let Err(err) = self.ctx.store.reset().await {
                        error!("error resetting session store: {err:}");
                        self.apply_transition(TransitionResult::TransitionTo(
                            SessionState::new_disconnected(false, "unexpected error in reset"),
                        ))
                        .await;
                    }
                }
                Err(err) => {
                    // actual schedule calculation error (e.g., DST transition, date overflow)
                    error!("error checking session period: {err:?}");
                    self.state
                        .logout_and_terminate(&mut self.ctx, "internal error")
                        .await;
                }
            }
        } else if self.state.is_connected() {
            // we are currently outside scheduled session time
            match self
                .state
                .initiate_graceful_logout(&mut self.ctx, "End of session time", true)
                .await
            {
                Ok(result) => self.apply_transition(result).await,
                Err(err) => error!(err = ?err, "failed to initiate graceful logout"),
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

        async fn on_state_change(&self, _from: &Status, _to: &Status) {}
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
