use crate::store::StoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Schedule configuration is invalid: {0}")]
    InvalidSchedule(String),

    #[error("store operation failed")]
    Store(#[from] StoreError),
}

pub type Result<T> = std::result::Result<T, SessionError>;

/// Outcome of a successful message send operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    /// Message was persisted and sent with the given sequence number.
    Sent { sequence_number: u64 },
    /// Message was dropped by the application callback.
    Dropped,
}

/// Error that can occur when sending a message.
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
