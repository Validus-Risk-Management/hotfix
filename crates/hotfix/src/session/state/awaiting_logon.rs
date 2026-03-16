use crate::transport::writer::WriterRef;
use tokio::time::Instant;

pub(crate) struct AwaitingLogonState {
    /// The writer's reference to send messages to the counterparty
    pub(crate) writer: WriterRef,
    /// Indicates whether we have sent Logon - safeguards against accidental double sends
    pub(crate) logon_sent: bool,
    /// When we are expecting the Logon response at the latest
    pub(crate) logon_timeout: Instant,
}
