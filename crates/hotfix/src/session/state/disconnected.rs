use crate::session::event::AwaitingActiveSessionResponse;
use tokio::sync::oneshot;

pub(crate) struct DisconnectedState {
    /// Indicates whether we should attempt to reconnect
    pub(crate) reconnect: bool,
    /// The channel for notifying the session loop when trading hours resume
    /// as indicated by the schedule
    session_awaiter: Option<oneshot::Sender<AwaitingActiveSessionResponse>>,
    /// The reason we are disconnected
    pub(crate) reason: String,
}

impl DisconnectedState {
    pub(crate) fn new(reconnect: bool, reason: &str) -> Self {
        Self {
            reconnect,
            session_awaiter: None,
            reason: reason.to_string(),
        }
    }

    pub(crate) fn set_session_awaiter(
        &mut self,
        responder: oneshot::Sender<AwaitingActiveSessionResponse>,
    ) {
        self.session_awaiter = Some(responder);
    }

    pub(crate) fn has_session_awaiter(&self) -> bool {
        self.session_awaiter.is_some()
    }

    pub(crate) fn take_session_awaiter(
        &mut self,
    ) -> Option<oneshot::Sender<AwaitingActiveSessionResponse>> {
        self.session_awaiter.take()
    }
}
