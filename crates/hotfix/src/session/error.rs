use crate::store::StoreError;
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

    #[error("confirmation channel closed")]
    ConfirmationLost,
}
