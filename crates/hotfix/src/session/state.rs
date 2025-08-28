use crate::message::parser::RawFixMessage;
use crate::session::event::AwaitingActiveSessionResponse;
use crate::session::info::Status as SessionInfoStatus;
use crate::transport::writer::WriterRef;
use hotfix_message::message::Message;
use std::collections::VecDeque;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::Instant;
use tracing::{debug, error};

const TEST_REQUEST_THRESHOLD: f64 = 1.2;

pub(crate) type TestRequestId = String;

pub enum SessionState {
    /// We have established a connection, sent a logon message and await a response.
    AwaitingLogon {
        writer: WriterRef,
        logon_sent: bool,
        logon_timeout: Instant,
    },
    /// We are awaiting the target to resend the gap we have.
    AwaitingResend(AwaitingResendState),
    /// We are in the process of gracefully logging out
    AwaitingLogout { writer: WriterRef }, // we need the writer so we can disconnect it on successful logout
    /// The session is active, we have connected and mutually logged on.
    Active(ActiveState),
    /// The peer has logged us out.
    LoggedOut { reconnect: bool },
    /// The TCP connection has been dropped.
    ///
    /// This is also the state we're in if we purposefully disconnected due to the current
    /// time being out of session hours.
    Disconnected(DisconnectedState),
}

impl SessionState {
    pub fn new_disconnected(reconnect: bool, reason: &str) -> Self {
        Self::Disconnected(DisconnectedState::new(reconnect, reason))
    }

    pub fn new_active(writer: WriterRef, heartbeat_interval: u64) -> Self {
        let peer_interval = calculate_peer_interval(heartbeat_interval);

        Self::Active(ActiveState {
            writer,
            heartbeat_deadline: Instant::now() + Duration::from_secs(heartbeat_interval),
            peer_deadline: Instant::now() + Duration::from_secs(peer_interval),
            sent_test_request_id: None,
        })
    }

    pub fn should_reconnect(&self) -> bool {
        match self {
            SessionState::Disconnected(DisconnectedState { reconnect, .. }) => *reconnect,
            _ => true,
        }
    }

    pub async fn send_message(&mut self, message_type: &[u8], message: RawFixMessage) {
        match self {
            Self::Active(ActiveState { writer, .. })
            | Self::AwaitingResend(AwaitingResendState { writer, .. }) => {
                if message_type == b"A" {
                    error!("logon message is invalid for active sessions")
                } else {
                    writer.send_raw_message(message).await
                }
            }
            Self::AwaitingLogon {
                writer, logon_sent, ..
            } => {
                match message_type {
                    b"A" => {
                        // Logon message
                        if *logon_sent {
                            error!("trying to send logon twice");
                        } else {
                            writer.send_raw_message(message).await;
                            *logon_sent = true;
                        }
                    }
                    b"5" => {
                        // Logout message
                        writer.send_raw_message(message).await;
                    }
                    _ => error!("invalid outgoing message for AwaitingLogon state"),
                }
            }
            Self::AwaitingLogout { writer } => {
                // Logout messages are allowed because we first transition into AwaitingLogout
                // and only then send the logout message
                if message_type == b"5" {
                    writer.send_raw_message(message).await
                }
            }
            _ => error!("trying to write without an established connection"),
        }
    }

    pub async fn disconnect(&self) {
        match self {
            Self::Active(ActiveState { writer, .. })
            | Self::AwaitingLogon { writer, .. }
            | Self::AwaitingLogout { writer }
            | Self::AwaitingResend(AwaitingResendState { writer, .. }) => writer.disconnect().await,
            _ => debug!("disconnecting an already disconnected session"),
        }
    }

    fn get_writer(&self) -> Option<&WriterRef> {
        match self {
            Self::Active(ActiveState { writer, .. })
            | Self::AwaitingLogon { writer, .. }
            | Self::AwaitingLogout { writer }
            | Self::AwaitingResend(AwaitingResendState { writer, .. }) => Some(writer),
            _ => None,
        }
    }

    pub fn try_transition_to_awaiting_logout(&mut self) -> bool {
        if matches!(self, SessionState::AwaitingLogout { .. }) {
            debug!("already in awaiting logout state");
            return false;
        }

        if let Some(writer) = self.get_writer() {
            *self = SessionState::AwaitingLogout {
                writer: writer.clone(),
            };
            true
        } else {
            error!("trying to transition to awaiting logout without an established connection");
            false
        }
    }

    pub fn try_transition_to_awaiting_resend(&mut self, end_seq_number: u64) -> bool {
        if matches!(self, SessionState::AwaitingLogout { .. }) {
            error!("trying to request a resend while we are already logging out");
            return false;
        }

        if let Some(writer) = self.get_writer() {
            let awaiting_resend = AwaitingResendState::new(writer.to_owned(), end_seq_number);
            *self = SessionState::AwaitingResend(awaiting_resend);
            true
        } else {
            error!("trying to transition to awaiting resend without an established connection");
            false
        }
    }

    pub fn register_session_awaiter(
        &mut self,
        responder: oneshot::Sender<AwaitingActiveSessionResponse>,
    ) {
        match self {
            SessionState::Disconnected(state) => {
                if state.has_session_awaiter() {
                    let reason = &state.reason;
                    error!(
                        "session awaiter already registered on state disconnected due to: {reason}"
                    );
                    if let Err(err) = responder.send(AwaitingActiveSessionResponse::Shutdown) {
                        error!("failed to send session awaiter response: {err:?}");
                    }
                } else {
                    state.set_session_awaiter(responder);
                    debug!("registered session awaiter");
                }
            }
            _ => {
                error!("session awaiter can only be registered on disconnected sessions");
                if let Err(err) = responder.send(AwaitingActiveSessionResponse::Shutdown) {
                    error!("failed to send session awaiter response: {err:?}");
                }
            }
        }
    }

    pub fn notify_session_awaiter(&mut self) {
        if let SessionState::Disconnected(state) = self
            && let Some(awaiter) = state.take_session_awaiter()
        {
            if let Err(err) = awaiter.send(AwaitingActiveSessionResponse::Active) {
                error!("failed to send session awaiter response: {err:?}");
            } else {
                debug!("notified session awaiter");
            }
        }
    }

    pub fn heartbeat_deadline(&self) -> Option<&Instant> {
        match self {
            Self::Active(ActiveState {
                heartbeat_deadline, ..
            }) => Some(heartbeat_deadline),
            _ => None,
        }
    }

    pub fn reset_heartbeat_timer(&mut self, heartbeat_interval: u64) {
        if let Self::Active(ActiveState {
            heartbeat_deadline, ..
        }) = self
        {
            *heartbeat_deadline = Instant::now() + Duration::from_secs(heartbeat_interval);
        }
    }

    pub fn peer_deadline(&self) -> Option<&Instant> {
        match self {
            Self::Active(ActiveState { peer_deadline, .. }) => Some(peer_deadline),
            Self::AwaitingLogon { logon_timeout, .. } => Some(logon_timeout),
            _ => None,
        }
    }

    pub fn reset_peer_timer(
        &mut self,
        heartbeat_interval: u64,
        test_request_id: Option<TestRequestId>,
    ) {
        if let Self::Active(ActiveState {
            peer_deadline,
            sent_test_request_id,
            ..
        }) = self
        {
            let interval = calculate_peer_interval(heartbeat_interval);
            *peer_deadline = Instant::now() + Duration::from_secs(interval);
            *sent_test_request_id = test_request_id;
        }
    }

    pub fn expected_test_response_id(&self) -> Option<&TestRequestId> {
        match self {
            Self::Active(ActiveState {
                sent_test_request_id: expected_test_response_id,
                ..
            }) => expected_test_response_id.as_ref(),
            _ => None,
        }
    }

    pub fn is_expecting_test_response(&self) -> bool {
        self.expected_test_response_id().is_some()
    }

    pub fn is_awaiting_logon(&self) -> bool {
        matches!(self, SessionState::AwaitingLogon { .. })
    }

    pub fn as_status(&self) -> SessionInfoStatus {
        match self {
            SessionState::AwaitingLogon { .. } => SessionInfoStatus::AwaitingLogon,
            SessionState::AwaitingResend(_) => SessionInfoStatus::AwaitingResend,
            SessionState::AwaitingLogout { .. } => SessionInfoStatus::AwaitingLogout,
            SessionState::Active(_) => SessionInfoStatus::Active,
            SessionState::LoggedOut { .. } => SessionInfoStatus::LoggedOut,
            SessionState::Disconnected(_) => SessionInfoStatus::Disconnected,
        }
    }
}

#[inline]
fn calculate_peer_interval(heartbeat_interval: u64) -> u64 {
    (heartbeat_interval as f64 * TEST_REQUEST_THRESHOLD).round() as u64
}

pub struct ActiveState {
    /// The writer's reference to send messages to the counterparty
    writer: WriterRef,
    /// When we should send the next heartbeat message to the counterparty
    heartbeat_deadline: Instant,
    /// When the next message from the counterparty is expected at the latest
    peer_deadline: Instant,
    /// The ID of the test request we sent on peer timer expiry
    sent_test_request_id: Option<TestRequestId>,
}

/// Session state we're in while processing messages we requested to be resent.
pub struct AwaitingResendState {
    /// The reference to the writer loop.
    pub(crate) writer: WriterRef,
    /// The end of the gap we're waiting for the target to resend.
    pub(crate) end_seq_number: u64,
    /// Inbound messages we receive while processing the resend.
    pub(crate) inbound_queue: VecDeque<Message>,
}

impl AwaitingResendState {
    pub fn new(writer: WriterRef, end_seq_number: u64) -> Self {
        Self {
            writer,
            end_seq_number,
            inbound_queue: Default::default(),
        }
    }
}

pub struct DisconnectedState {
    reconnect: bool,
    session_awaiter: Option<oneshot::Sender<AwaitingActiveSessionResponse>>,
    reason: String,
}

impl DisconnectedState {
    fn new(reconnect: bool, reason: &str) -> Self {
        Self {
            reconnect,
            session_awaiter: None,
            reason: reason.to_string(),
        }
    }

    fn set_session_awaiter(&mut self, responder: oneshot::Sender<AwaitingActiveSessionResponse>) {
        self.session_awaiter = Some(responder);
    }

    fn has_session_awaiter(&self) -> bool {
        self.session_awaiter.is_some()
    }

    fn take_session_awaiter(&mut self) -> Option<oneshot::Sender<AwaitingActiveSessionResponse>> {
        self.session_awaiter.take()
    }
}
