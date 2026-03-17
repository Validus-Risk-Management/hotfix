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
    /// Ask the session to notify us when the schedule indicates we should connect.
    AwaitSchedule(oneshot::Sender<ScheduleResponse>),
}

/// The response sent by the session to AwaitSchedule messages.
///
/// This doesn't include an out-of-schedule variant, as the session won't respond
/// until the schedule indicates we should connect or the session is in a state that
/// indicates it should just be shut down due to an unrecoverable error.
#[derive(Debug, Clone, Copy)]
pub enum ScheduleResponse {
    /// The schedule indicates we should connect.
    InSchedule,
    /// The session should be shut down due to an unrecoverable error.
    Shutdown,
}
