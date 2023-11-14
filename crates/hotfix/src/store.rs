pub mod in_memory;
#[cfg(feature = "mongodb")]
pub mod mongodb;
#[cfg(feature = "redb")]
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
