use crate::session::SessionInfo;
use tokio::sync::oneshot;

/// Administrative actions exposed to users of the engine to control the session.
pub enum AdminRequest {
    /// Ask the session to shut down.
    InitiateGracefulShutdown { reconnect: bool },
    /// Ask the session for a report on its state
    RequestSessionInfo(oneshot::Sender<SessionInfo>),
    /// Set the session to reset sequence numbers on the next logon as a one-off.
    ///
    /// This is an override for the configuration's persistent setting for `ResetOnLogon`,
    /// which can be used to re-synchronise our state with the counterparty in
    /// unfortunate scenarios where such drastic recover is required.
    ResetSequenceNumbersOnNextLogon,
}
