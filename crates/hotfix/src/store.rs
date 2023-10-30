pub mod in_memory;
#[cfg(feature = "redb")]
pub mod redb;

#[async_trait::async_trait]
pub trait MessageStore {
    async fn add(&mut self, sequence_number: u64, message: &[u8]);
    async fn get_slice(&self, begin: usize, end: usize) -> Vec<Vec<u8>>;
    async fn next_sender_seq_number(&self) -> u64;
    async fn next_target_seq_number(&self) -> u64;
    async fn increment_sender_seq_number(&mut self);
    async fn increment_target_seq_number(&mut self);
    async fn reset(&mut self);
}
