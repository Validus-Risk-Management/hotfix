use crate::store::MessageStore;
use anyhow::Result;
use chrono::{DateTime, Utc};

#[derive(Debug)]
pub struct InMemoryMessageStore {
    sender_seq_number: u64,
    target_seq_number: u64,
    creation_time: DateTime<Utc>,
    messages: Vec<Vec<u8>>,
}

impl Default for InMemoryMessageStore {
    fn default() -> Self {
        Self {
            sender_seq_number: 0,
            target_seq_number: 0,
            creation_time: Utc::now(),
            messages: vec![],
        }
    }
}

#[async_trait::async_trait]
impl MessageStore for InMemoryMessageStore {
    async fn add(&mut self, sequence_number: u64, message: &[u8]) -> Result<()> {
        assert_eq!(sequence_number as usize, self.messages.len());
        self.messages.push(message.to_vec());
        Ok(())
    }

    async fn get_slice(&self, begin: usize, end: usize) -> Result<Vec<Vec<u8>>> {
        Ok(self.messages.as_slice()[begin..=end].to_vec())
    }

    fn next_sender_seq_number(&self) -> u64 {
        self.sender_seq_number + 1
    }

    fn next_target_seq_number(&self) -> u64 {
        self.target_seq_number + 1
    }

    async fn increment_sender_seq_number(&mut self) -> Result<()> {
        self.sender_seq_number += 1;
        Ok(())
    }

    async fn increment_target_seq_number(&mut self) -> Result<()> {
        self.target_seq_number += 1;
        Ok(())
    }

    async fn set_target_seq_number(&mut self, seq_number: u64) -> Result<()> {
        self.target_seq_number = seq_number;
        Ok(())
    }

    async fn reset(&mut self) -> Result<()> {
        self.sender_seq_number = 0;
        self.target_seq_number = 0;
        self.messages.clear();
        self.creation_time = Utc::now();
        Ok(())
    }

    async fn creation_time(&self) -> Result<DateTime<Utc>> {
        Ok(self.creation_time)
    }
}
