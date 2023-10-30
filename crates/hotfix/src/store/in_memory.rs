use crate::store::MessageStore;

#[derive(Debug, Default)]
pub struct InMemoryMessageStore {
    sender_seq_number: u64,
    target_seq_number: u64,
    messages: Vec<Vec<u8>>,
}

#[async_trait::async_trait]
impl MessageStore for InMemoryMessageStore {
    async fn add(&mut self, sequence_number: u64, message: &[u8]) {
        assert_eq!(sequence_number as usize, self.messages.len());
        self.messages.push(message.to_vec());
    }

    async fn get_slice(&self, begin: usize, end: usize) -> Vec<Vec<u8>> {
        self.messages.as_slice()[begin..=end].to_vec()
    }

    async fn next_sender_seq_number(&self) -> u64 {
        self.sender_seq_number + 1
    }

    async fn next_target_seq_number(&self) -> u64 {
        self.target_seq_number + 1
    }

    async fn increment_sender_seq_number(&mut self) {
        self.sender_seq_number += 1;
    }

    async fn increment_target_seq_number(&mut self) {
        self.target_seq_number += 1;
    }

    async fn reset(&mut self) {
        self.sender_seq_number = 0;
        self.target_seq_number = 0;
        self.messages.clear();
    }
}
