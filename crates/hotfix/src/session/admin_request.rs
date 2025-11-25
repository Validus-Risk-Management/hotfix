use crate::session::SessionInfo;
use tokio::sync::oneshot;

/// Administrative actions exposed to users of the engine to control the session.
pub enum AdminRequest {
    /// Ask the session to shut down.
    InitiateGracefulShutdown { reconnect: bool },
    /// Ask the session for a report on its state
    RequestSessionInfo(oneshot::Sender<SessionInfo>),
    /// Set the session to reset sequence numbers on the next logon as a one-off.
    ResetSequenceNumbersOnNextLogon,
}
