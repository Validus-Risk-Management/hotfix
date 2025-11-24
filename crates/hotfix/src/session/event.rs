use tokio::sync::oneshot;

use crate::message::parser::RawFixMessage;
use crate::transport::writer::WriterRef;

#[derive(Debug)]
pub enum SessionEvent {
    /// Tell the session we have received a new FIX message from the reader.
    FixMessageReceived(RawFixMessage),
    /// Let the session know we've been disconnected.
    Disconnected(String),
    /// Register a new writer connected to the other side.
    Connected(WriterRef),
    /// Ask the session whether we should attempt to reconnect.
    ShouldReconnect(oneshot::Sender<bool>),
    /// Ask the session to notify us when the session is active.
    AwaitingActiveSession(oneshot::Sender<AwaitingActiveSessionResponse>),
}

/// The response sent by the session to AwaitingActiveSession messages.
///
/// This doesn't include an Inactive variant, as the session won't respond until
/// it's active or in a state that indicates it should just be shut down due to an
/// unrecoverable error.
#[derive(Debug, Clone, Copy)]
pub enum AwaitingActiveSessionResponse {
    /// The session is now active and ready to connect.
    Active,
    /// The session should be shut down due to an unrecoverable error.
    Shutdown,
}
