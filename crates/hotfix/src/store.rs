//! Message store implementations (re-exported from hotfix-store).
//!
//! By default, only the [in_memory] store is included. Further message store implementations,
//! such as `mongodb` can be enabled through feature flags.

/// Error types for store operations.
pub use hotfix_store::error;

/// An in-memory message store that loses its state on restart. Only use this for testing.
pub use hotfix_store::in_memory;

/// A file-based message store for persistence.
pub use hotfix_store::file;

#[cfg(feature = "mongodb")]
/// A message store using MongoDB for persistence.
pub use hotfix_store_mongodb as mongodb;

#[cfg(feature = "test-utils")]
/// Test utilities for message store implementations.
pub use hotfix_store::test_utils;

pub use hotfix_store::error::*;
pub use hotfix_store::{FileStore, InMemoryMessageStore, MessageStore};
