/// Information about the session's current state.
///
/// This is intended for external code to peek inside
/// the session's internals for debugging and monitoring.
#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub next_sender_seq_number: u64,
    pub next_target_seq_number: u64,
    pub status: Status,
}

/// The status of the session as reported to external consumers.
///
/// These roughly correspond to the `SessionState` variants but don't contain
/// internal state.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    AwaitingLogon,
    AwaitingResend,
    AwaitingLogout,
    Active,
    LoggedOut,
    Disconnected,
}
