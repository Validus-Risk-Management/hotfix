use anyhow::Result;
use redb::{Database, ReadableTable, TableDefinition};
use std::path::Path;

use crate::store::MessageStore;

const MESSAGES_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("messages");
const SEQ_NUMBER_TABLE: TableDefinition<&str, u64> = TableDefinition::new("seq_numbers");
const SENDER_KEY: &str = "sender";
const TARGET_KEY: &str = "target";

pub struct RedbMessageStore {
    db: Database,
    sender_seq_number: u64,
    target_seq_number: u64,
}

impl RedbMessageStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = Database::create(path)?;
        let sender_seq_number;
        let target_seq_number;

        {
            let read_txn = db.begin_read()?;
            {
                let table = read_txn.open_table(SEQ_NUMBER_TABLE)?;
                sender_seq_number = table.get(SENDER_KEY)?.map_or(0, |g| g.value());
                target_seq_number = table.get(TARGET_KEY)?.map_or(0, |g| g.value());
            }
        }

        Ok(Self {
            db,
            sender_seq_number,
            target_seq_number,
        })
    }
}

#[async_trait::async_trait]
impl MessageStore for RedbMessageStore {
    async fn add(&mut self, sequence_number: u64, message: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MESSAGES_TABLE)?;
            table.insert(sequence_number, message)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    async fn get_slice(&self, begin: usize, end: usize) -> Result<Vec<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        {
            let table = read_txn.open_table(MESSAGES_TABLE)?;
            let messages: std::result::Result<Vec<Vec<u8>>, redb::StorageError> = table
                .range(begin as u64..=end as u64)?
                .map(|m| m.map(|v| v.1.value().to_vec()))
                .collect();
            Ok(messages?)
        }
    }

    fn next_sender_seq_number(&self) -> u64 {
        self.sender_seq_number + 1
    }

    fn next_target_seq_number(&self) -> u64 {
        self.target_seq_number + 1
    }

    async fn increment_sender_seq_number(&mut self) -> Result<()> {
        self.sender_seq_number += 1;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SEQ_NUMBER_TABLE)?;
            table.insert(SENDER_KEY, self.sender_seq_number)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    async fn increment_target_seq_number(&mut self) -> Result<()> {
        self.target_seq_number += 1;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SEQ_NUMBER_TABLE)?;
            table.insert(TARGET_KEY, self.target_seq_number)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    async fn set_target_seq_number(&mut self, seq_number: u64) -> Result<()> {
        self.target_seq_number = seq_number;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SEQ_NUMBER_TABLE)?;
            table.insert(TARGET_KEY, seq_number)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    async fn reset(&mut self) -> Result<()> {
        self.sender_seq_number = 0;
        self.target_seq_number = 0;
        let write_txn = self.db.begin_write()?;
        {
            let mut seq_no_table = write_txn.open_table(SEQ_NUMBER_TABLE)?;
            seq_no_table.insert(SENDER_KEY, self.sender_seq_number)?;
            seq_no_table.insert(TARGET_KEY, self.target_seq_number)?;
            let mut messages_table = write_txn.open_table(MESSAGES_TABLE)?;
            messages_table.drain::<u64>(..)?;
        }
        write_txn.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use tokio;

    struct TestStore {
        store: RedbMessageStore,
        db_path: PathBuf,
    }

    impl TestStore {
        fn new() -> Self {
            let mut temp_path = env::temp_dir();
            temp_path.push(format!("redb_test_{}", uuid::Uuid::new_v4()));
            temp_path.set_extension("db");

            let store = RedbMessageStore::new(&temp_path).expect("Failed to create store");

            Self {
                store,
                db_path: temp_path,
            }
        }

        fn store(&self) -> &RedbMessageStore {
            &self.store
        }

        fn store_mut(&mut self) -> &mut RedbMessageStore {
            &mut self.store
        }

        fn db_path(&self) -> &PathBuf {
            &self.db_path
        }
    }

    impl Drop for TestStore {
        fn drop(&mut self) {
            // Clean up the database file when the test store is dropped
            let _ = fs::remove_file(&self.db_path);
        }
    }

    #[tokio::test]
    async fn test_new_store_initialization() {
        let test_store = TestStore::new();
        let store = test_store.store();

        assert_eq!(store.next_sender_seq_number(), 1);
        assert_eq!(store.next_target_seq_number(), 1);
    }

    #[tokio::test]
    async fn test_add_and_get_messages() {
        let mut test_store = TestStore::new();
        let store = test_store.store_mut();

        let message1 = b"test message 1";
        let message2 = b"test message 2";
        let message3 = b"test message 3";

        store
            .add(1, message1)
            .await
            .expect("Failed to add message 1");
        store
            .add(2, message2)
            .await
            .expect("Failed to add message 2");
        store
            .add(3, message3)
            .await
            .expect("Failed to add message 3");

        let messages = store.get_slice(1, 3).await.expect("Failed to get messages");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0], message1);
        assert_eq!(messages[1], message2);
        assert_eq!(messages[2], message3);
    }

    #[tokio::test]
    async fn test_get_slice_partial_range() {
        let mut test_store = TestStore::new();
        let store = test_store.store_mut();

        let message1 = b"message 1";
        let message2 = b"message 2";
        let message3 = b"message 3";
        let message4 = b"message 4";

        store
            .add(1, message1)
            .await
            .expect("Failed to add message 1");
        store
            .add(2, message2)
            .await
            .expect("Failed to add message 2");
        store
            .add(3, message3)
            .await
            .expect("Failed to add message 3");
        store
            .add(4, message4)
            .await
            .expect("Failed to add message 4");

        let messages = store.get_slice(2, 3).await.expect("Failed to get messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], message2);
        assert_eq!(messages[1], message3);
    }

    #[tokio::test]
    async fn test_get_slice_empty_range() {
        let test_store = TestStore::new();
        let store = test_store.store();

        let messages = store.get_slice(1, 3).await.expect("Failed to get messages");
        assert_eq!(messages.len(), 0);
    }

    #[tokio::test]
    async fn test_increment_sender_seq_number() {
        let mut test_store = TestStore::new();
        let store = test_store.store_mut();

        assert_eq!(store.next_sender_seq_number(), 1);

        store
            .increment_sender_seq_number()
            .await
            .expect("Failed to increment sender seq number");
        assert_eq!(store.next_sender_seq_number(), 2);

        store
            .increment_sender_seq_number()
            .await
            .expect("Failed to increment sender seq number");
        assert_eq!(store.next_sender_seq_number(), 3);
    }

    #[tokio::test]
    async fn test_increment_target_seq_number() {
        let mut test_store = TestStore::new();
        let store = test_store.store_mut();

        assert_eq!(store.next_target_seq_number(), 1);

        store
            .increment_target_seq_number()
            .await
            .expect("Failed to increment target seq number");
        assert_eq!(store.next_target_seq_number(), 2);

        store
            .increment_target_seq_number()
            .await
            .expect("Failed to increment target seq number");
        assert_eq!(store.next_target_seq_number(), 3);
    }

    #[tokio::test]
    async fn test_set_target_seq_number() {
        let mut test_store = TestStore::new();
        let store = test_store.store_mut();

        assert_eq!(store.next_target_seq_number(), 1);

        store
            .set_target_seq_number(10)
            .await
            .expect("Failed to set target seq number");
        assert_eq!(store.next_target_seq_number(), 11);

        store
            .set_target_seq_number(5)
            .await
            .expect("Failed to set target seq number");
        assert_eq!(store.next_target_seq_number(), 6);
    }

    #[tokio::test]
    async fn test_reset_store() {
        let mut test_store = TestStore::new();
        let store = test_store.store_mut();

        // Add some messages and increment sequence numbers
        store
            .add(1, b"message 1")
            .await
            .expect("Failed to add message");
        store
            .add(2, b"message 2")
            .await
            .expect("Failed to add message");
        store
            .increment_sender_seq_number()
            .await
            .expect("Failed to increment sender seq number");
        store
            .increment_target_seq_number()
            .await
            .expect("Failed to increment target seq number");

        assert_eq!(store.next_sender_seq_number(), 2);
        assert_eq!(store.next_target_seq_number(), 2);

        let messages_before_reset = store.get_slice(1, 2).await.expect("Failed to get messages");
        assert_eq!(messages_before_reset.len(), 2);

        // Reset the store
        store.reset().await.expect("Failed to reset store");

        // Verify sequence numbers are reset
        assert_eq!(store.next_sender_seq_number(), 1);
        assert_eq!(store.next_target_seq_number(), 1);

        // Verify messages are cleared
        let messages_after_reset = store.get_slice(1, 2).await.expect("Failed to get messages");
        assert_eq!(messages_after_reset.len(), 0);
    }

    #[tokio::test]
    async fn test_persistence_across_store_instances() {
        let test_store = TestStore::new();
        let db_path = test_store.db_path().clone();

        // Create first store instance and add data
        {
            let mut store1 = RedbMessageStore::new(&db_path).expect("Failed to create store1");
            store1
                .add(1, b"persistent message")
                .await
                .expect("Failed to add message");
            store1
                .increment_sender_seq_number()
                .await
                .expect("Failed to increment sender seq number");
            store1
                .set_target_seq_number(5)
                .await
                .expect("Failed to set target seq number");
        }

        // Create second store instance and verify data persists
        {
            let store2 = RedbMessageStore::new(&db_path).expect("Failed to create store2");

            assert_eq!(store2.next_sender_seq_number(), 2);
            assert_eq!(store2.next_target_seq_number(), 6);

            let messages = store2
                .get_slice(1, 1)
                .await
                .expect("Failed to get messages");
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0], b"persistent message");
        }

        // TestStore will clean up the file when it goes out of scope
    }

    #[tokio::test]
    async fn test_add_messages_non_sequential() {
        let mut test_store = TestStore::new();
        let store = test_store.store_mut();

        // Add messages in non-sequential order
        store
            .add(5, b"message 5")
            .await
            .expect("Failed to add message 5");
        store
            .add(1, b"message 1")
            .await
            .expect("Failed to add message 1");
        store
            .add(3, b"message 3")
            .await
            .expect("Failed to add message 3");

        let messages = store.get_slice(1, 5).await.expect("Failed to get messages");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0], b"message 1");
        assert_eq!(messages[1], b"message 3");
        assert_eq!(messages[2], b"message 5");
    }

    #[tokio::test]
    async fn test_get_slice_beyond_available_messages() {
        let mut test_store = TestStore::new();
        let store = test_store.store_mut();

        store
            .add(1, b"only message")
            .await
            .expect("Failed to add message");

        let messages = store
            .get_slice(1, 10)
            .await
            .expect("Failed to get messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], b"only message");
    }

    #[tokio::test]
    async fn test_overwrite_existing_message() {
        let mut test_store = TestStore::new();
        let store = test_store.store_mut();

        store
            .add(1, b"original message")
            .await
            .expect("Failed to add original message");
        store
            .add(1, b"updated message")
            .await
            .expect("Failed to add updated message");

        let messages = store.get_slice(1, 1).await.expect("Failed to get messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], b"updated message");
    }
}
