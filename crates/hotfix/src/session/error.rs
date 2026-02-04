use crate::store::StoreError;
use hotfix_message::error::EncodingError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("store operation failed")]
    Store(#[from] StoreError),
}

#[derive(Debug, Error)]
pub enum SessionCreationError {
    #[error("unsupported BeginString: {0}")]
    UnsupportedBeginString(String),

    #[error("dictionary failed to parse")]
    MalformedDictionary(#[from] hotfix_message::dict::ParseError),

    #[error("dictionary contents are invalid")]
    InvalidDictionary(#[from] hotfix_message::error::ParserError),

    #[error("schedule configuration is invalid: {0}")]
    InvalidSchedule(String),
}

/// Outcome of a successful message send operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    /// Message was persisted and sent with the given sequence number.
    Sent { sequence_number: u64 },
    /// Message was dropped by the application callback.
    Dropped,
}

/// Error that can occur when sending an outbound message to the session.
#[derive(Debug, Error)]
pub enum SendError {
    #[error("session is disconnected")]
    Disconnected,

    #[error("failed to persist message")]
    Persist(#[source] StoreError),

    #[error("failed to update sequence number")]
    SequenceNumber(#[source] StoreError),

    #[error("session terminated by application")]
    SessionTerminated,

    /// The session task is no longer running.
    #[error("session is no longer available")]
    SessionGone,
}

/// Error that can occur when sending a message internally within the session.
///
/// This is a subset of `SendError` without `SessionTerminated` and `SessionGone`,
/// which only make sense in the context of the public API.
#[derive(Debug, Error)]
pub(crate) enum InternalSendError {
    /// The session is disconnected. Currently unused as internal sends don't
    /// check connection state, but kept for symmetry with `SendError`.
    #[allow(dead_code)]
    #[error("session is disconnected")]
    Disconnected,

    #[error("failed to persist message")]
    Persist(#[source] StoreError),

    #[error("failed to update sequence number")]
    SequenceNumber(#[source] StoreError),
}

impl From<InternalSendError> for SendError {
    fn from(err: InternalSendError) -> Self {
        match err {
            InternalSendError::Disconnected => SendError::Disconnected,
            InternalSendError::Persist(e) => SendError::Persist(e),
            InternalSendError::SequenceNumber(e) => SendError::SequenceNumber(e),
        }
    }
}

/// Error that can occur during internal session operations.
///
/// This replaces anyhow::Context wrapping with structured error variants.
#[derive(Debug, Error)]
pub(crate) enum SessionOperationError {
    /// Failed to send a message.
    #[error("failed to send {context}")]
    Send {
        #[source]
        source: InternalSendError,
        context: &'static str,
    },

    /// A store operation failed.
    #[error("store operation failed")]
    Store(#[source] StoreError),

    /// Failed to encode a message.
    #[error("failed to encode message")]
    MessageEncoding(#[source] EncodingError),

    /// Failed to parse a stored message.
    #[error("failed to parse stored message: {0}")]
    StoredMessageParse(String),

    /// A required field was missing from a message.
    #[error("missing required field: {0}")]
    MissingField(&'static str),
}
