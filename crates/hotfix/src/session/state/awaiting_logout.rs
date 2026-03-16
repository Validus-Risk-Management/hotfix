use crate::transport::writer::WriterRef;
use tokio::time::Instant;

pub(crate) struct AwaitingLogoutState {
    /// The writer's reference to send messages to the counterparty
    pub(crate) writer: WriterRef,
    /// When we are expecting the Logout response at the latest
    pub(crate) logout_timeout: Instant,
    /// Indicates whether we should attempt to reconnect after we've fully logged out
    pub(crate) reconnect: bool,
}
