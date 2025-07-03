use crate::store::MessageStore;
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug)]
pub struct InMemoryMessageStore {
    sender_seq_number: u64,
    target_seq_number: u64,
    creation_time: DateTime<Utc>,
    messages: HashMap<u64, Vec<u8>>,
}

impl Default for InMemoryMessageStore {
    fn default() -> Self {
        Self {
            sender_seq_number: 0,
            target_seq_number: 0,
            creation_time: Utc::now(),
            messages: HashMap::new(),
        }
    }
}

#[async_trait::async_trait]
impl MessageStore for InMemoryMessageStore {
    async fn add(&mut self, sequence_number: u64, message: &[u8]) -> Result<()> {
        self.messages.insert(sequence_number, message.to_vec());
        Ok(())
    }

    async fn get_slice(&self, begin: usize, end: usize) -> Result<Vec<Vec<u8>>> {
        let mut msgs = Vec::with_capacity(end - begin + 1);
        for idx in begin..=end {
            if let Some(msg) = self.messages.get(&(idx as u64)) {
                msgs.push(msg.to_vec());
            }
        }
        Ok(msgs)
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

#[cfg(test)]
pub(crate) mod test_utils {
    use super::*;
    use crate::store::tests::TestStoreFactory;

    pub(crate) struct InMemoryMessageStoreTestFactory;

    impl TestStoreFactory for InMemoryMessageStoreTestFactory {
        fn create_store(&self) -> Box<dyn MessageStore> {
            Box::new(InMemoryMessageStore::default())
        }

        fn is_persistent(&self) -> bool {
            false
        }
    }
}
