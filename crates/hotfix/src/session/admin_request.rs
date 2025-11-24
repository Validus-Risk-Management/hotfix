use crate::session::SessionInfo;
use tokio::sync::oneshot;

pub enum AdminRequest {
    /// Ask the session to shut down.
    RequestGracefulShutdown,
    /// Ask the session for a report on its state
    RequestSessionInfo(oneshot::Sender<SessionInfo>),
}
