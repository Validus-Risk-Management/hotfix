use crate::session::event::ScheduleResponse;
use tokio::sync::oneshot;

pub(crate) struct DisconnectedState {
    /// Indicates whether we should attempt to reconnect
    pub(crate) reconnect: bool,
    /// The channel for notifying the session loop when trading hours resume
    /// as indicated by the schedule
    schedule_awaiter: Option<oneshot::Sender<ScheduleResponse>>,
    /// The reason we are disconnected
    pub(crate) reason: String,
}

impl DisconnectedState {
    pub(crate) fn new(reconnect: bool, reason: &str) -> Self {
        Self {
            reconnect,
            schedule_awaiter: None,
            reason: reason.to_string(),
        }
    }

    pub(crate) fn set_schedule_awaiter(&mut self, responder: oneshot::Sender<ScheduleResponse>) {
        self.schedule_awaiter = Some(responder);
    }

    pub(crate) fn has_schedule_awaiter(&self) -> bool {
        self.schedule_awaiter.is_some()
    }

    pub(crate) fn take_schedule_awaiter(&mut self) -> Option<oneshot::Sender<ScheduleResponse>> {
        self.schedule_awaiter.take()
    }
}
