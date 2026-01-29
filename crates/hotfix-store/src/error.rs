//! Error types for message store operations.

use thiserror::Error;

/// A boxed error type for store errors.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Errors that can occur during message store operations.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Failed to initialize the store.
    #[error("failed to initialize store: {0}")]
    Initialization(#[source] BoxError),

    /// Failed to persist a message to the store.
    #[error("failed to persist message (seq_num: {sequence_number})")]
    PersistMessage {
        sequence_number: u64,
        #[source]
        source: BoxError,
    },

    /// Failed to retrieve messages from the store.
    #[error("failed to retrieve messages (range: {begin}..={end})")]
    RetrieveMessages {
        begin: usize,
        end: usize,
        #[source]
        source: BoxError,
    },

    /// Failed to update a sequence number.
    #[error("failed to update sequence number")]
    UpdateSequenceNumber(#[source] BoxError),

    /// Failed to reset the store.
    #[error("failed to reset store")]
    Reset(#[source] BoxError),

    /// Failed to cleanup old sequences.
    #[error("failed to cleanup old sequences")]
    Cleanup(#[source] BoxError),
}

/// A specialized Result type for store operations.
pub type Result<T> = std::result::Result<T, StoreError>;
