//! Message store traits and implementations for the hotfix FIX engine.
//!
//! This crate provides the [`MessageStore`] trait and several implementations:
//!
//! - [`InMemoryMessageStore`]: An in-memory store for testing (loses state on restart)
//! - [`FileStore`]: A file-based store for persistence
//!
//! # Features
//!
//! - `test-utils`: Enables the [`test_utils`] module with test harness for store implementations

/// Error types for store operations.
pub mod error;

/// File-based message store for persistence.
pub mod file;

/// In-memory message store (non-persistent).
pub mod in_memory;

/// Test utilities for message store implementations.
#[cfg(feature = "test-utils")]
pub mod test_utils;

pub use error::{BoxError, Result, StoreError};
pub use file::FileStore;
pub use in_memory::InMemoryMessageStore;

use chrono::DateTime;

/// A trait for storing and retrieving FIX messages and sequence numbers.
///
/// Message stores are responsible for:
/// - Persisting outgoing messages for potential resend
/// - Tracking sender and target sequence numbers
/// - Storing session creation time
///
/// Implementations should be async-safe and handle concurrent access appropriately.
#[async_trait::async_trait]
pub trait MessageStore: Send + Sync {
    /// Adds a message to the store with the given sequence number.
    async fn add(&mut self, sequence_number: u64, message: &[u8]) -> Result<()>;

    /// Retrieves messages in the given sequence number range (inclusive).
    async fn get_slice(&self, begin: usize, end: usize) -> Result<Vec<Vec<u8>>>;

    /// Returns the next sender sequence number (current + 1).
    fn next_sender_seq_number(&self) -> u64;

    /// Returns the next target sequence number (current + 1).
    fn next_target_seq_number(&self) -> u64;

    /// Increments the sender sequence number by 1.
    async fn increment_sender_seq_number(&mut self) -> Result<()>;

    /// Increments the target sequence number by 1.
    async fn increment_target_seq_number(&mut self) -> Result<()>;

    /// Sets the target sequence number to a specific value.
    async fn set_target_seq_number(&mut self, seq_number: u64) -> Result<()>;

    /// Resets the store, clearing all messages and resetting sequence numbers.
    async fn reset(&mut self) -> Result<()>;

    /// Returns the creation time of the current session.
    fn creation_time(&self) -> DateTime<chrono::Utc>;
}
