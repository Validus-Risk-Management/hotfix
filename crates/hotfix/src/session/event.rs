use tokio::sync::oneshot;

use crate::message::parser::RawFixMessage;
use crate::transport::socket_writer::WriterRef;

#[derive(Debug)]
pub enum SessionEvent<M> {
    /// Tell the session we have received a new FIX message from the reader.
    FixMessageReceived(RawFixMessage),
    /// Ask the session to send a message from the application.
    SendMessage(M),
    /// Let the session know we've been disconnected.
    Disconnected(String),
    /// Register a new writer connected to the other side.
    Connected(WriterRef),
    /// Ask the session whether we should attempt to reconnect.
    ShouldReconnect(oneshot::Sender<bool>),
}
