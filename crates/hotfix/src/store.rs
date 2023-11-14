//! Implementations of in-memory and persistent message stores holding session state.
//!
//! By default, only the [in_memory] store is included. Further message store implementations,
//! such as `mongodb` and `redb` can be enabled through feature flags.

/// An in-memory message store that loses its state on restart. Only use this for testing.
pub mod in_memory;

#[cfg(feature = "mongodb")]
/// A message store using MongoDB for persistence.
pub mod mongodb;

#[cfg(feature = "redb")]
/// A message store using [redb](https://www.redb.org/) for persistence.
pub mod redb;

use anyhow::Result;

#[async_trait::async_trait]
pub trait MessageStore {
    async fn add(&mut self, sequence_number: u64, message: &[u8]) -> Result<()>;
    async fn get_slice(&self, begin: usize, end: usize) -> Result<Vec<Vec<u8>>>;
    async fn next_sender_seq_number(&self) -> u64;
    async fn next_target_seq_number(&self) -> u64;
    async fn increment_sender_seq_number(&mut self) -> Result<()>;
    async fn increment_target_seq_number(&mut self) -> Result<()>;
    async fn set_target_seq_number(&mut self, seq_number: u64) -> Result<()>;
    async fn reset(&mut self) -> Result<()>;
}
