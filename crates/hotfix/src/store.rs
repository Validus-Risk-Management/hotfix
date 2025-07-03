//! Implementations of in-memory and persistent message stores holding session state.
//!
//! By default, only the [in_memory] store is included. Further message store implementations,
//! such as `mongodb` and `redb` can be enabled through feature flags.

/// An in-memory message store that loses its state on restart. Only use this for testing.
pub mod in_memory;

#[cfg(feature = "dynamodb")]
pub mod dynamodb;

#[cfg(feature = "mongodb")]
/// A message store using MongoDB for persistence.
pub mod mongodb;

#[cfg(feature = "redb")]
/// A message store using [redb](https://www.redb.org/) for persistence.
pub mod redb;

use anyhow::Result;
use chrono::DateTime;

#[async_trait::async_trait]
pub trait MessageStore {
    async fn add(&mut self, sequence_number: u64, message: &[u8]) -> Result<()>;
    async fn get_slice(&self, begin: usize, end: usize) -> Result<Vec<Vec<u8>>>;
    fn next_sender_seq_number(&self) -> u64;
    fn next_target_seq_number(&self) -> u64;
    async fn increment_sender_seq_number(&mut self) -> Result<()>;
    async fn increment_target_seq_number(&mut self) -> Result<()>;
    async fn set_target_seq_number(&mut self, seq_number: u64) -> Result<()>;
    async fn reset(&mut self) -> Result<()>;
    async fn creation_time(&self) -> Result<DateTime<chrono::Utc>>;
}

#[cfg(test)]
mod tests {
    use super::in_memory::test_utils::InMemoryMessageStoreTestFactory;
    use super::*;
    use tokio;

    pub trait TestStoreFactory {
        fn create_store(&self) -> Box<dyn MessageStore>;
        fn is_persistent(&self) -> bool {
            true
        }
    }

    fn create_test_store_factories() -> Vec<Box<dyn TestStoreFactory>> {
        let mut stores: Vec<Box<dyn TestStoreFactory>> = Vec::new();

        // Add in-memory store factory
        stores.push(Box::new(InMemoryMessageStoreTestFactory {}) as Box<dyn TestStoreFactory>);

        // Add redb store factory if the feature is enabled
        #[cfg(feature = "redb")]
        {
            use super::redb::test_utils::RedbTestStoreFactory;
            stores.push(Box::new(RedbTestStoreFactory::new()) as Box<dyn TestStoreFactory>);
        }

        stores
    }

    #[tokio::test]
    async fn test_new_store_initialization() {
        for factory in create_test_store_factories() {
            let store = factory.create_store();

            assert_eq!(store.next_sender_seq_number(), 1);
            assert_eq!(store.next_target_seq_number(), 1);
        }
    }

    #[tokio::test]
    async fn test_add_and_get_messages() {
        for factory in create_test_store_factories() {
            let mut store = factory.create_store();

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
    }

    #[tokio::test]
    async fn test_get_slice_partial_range() {
        for factory in create_test_store_factories() {
            let mut store = factory.create_store();

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
    }

    #[tokio::test]
    async fn test_get_slice_empty_range() {
        for factory in create_test_store_factories() {
            let store = factory.create_store();

            let messages = store.get_slice(1, 3).await.expect("Failed to get messages");
            assert_eq!(messages.len(), 0);
        }
    }

    #[tokio::test]
    async fn test_increment_sender_seq_number() {
        for factory in create_test_store_factories() {
            let mut store = factory.create_store();

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
    }

    #[tokio::test]
    async fn test_increment_target_seq_number() {
        for factory in create_test_store_factories() {
            let mut store = factory.create_store();

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
    }

    #[tokio::test]
    async fn test_set_target_seq_number() {
        for factory in create_test_store_factories() {
            let mut store = factory.create_store();

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
    }

    #[tokio::test]
    async fn test_reset_store() {
        for factory in create_test_store_factories() {
            let mut store = factory.create_store();

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

            let messages_before_reset =
                store.get_slice(1, 2).await.expect("Failed to get messages");
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
    }

    #[tokio::test]
    async fn test_persistence_across_store_instances() {
        for factory in create_test_store_factories() {
            if !factory.is_persistent() {
                continue;
            }

            // Create first store instance and add data
            {
                let mut store1 = factory.create_store();
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
                let store2 = factory.create_store();

                assert_eq!(store2.next_sender_seq_number(), 2);
                assert_eq!(store2.next_target_seq_number(), 6);

                let messages = store2
                    .get_slice(1, 1)
                    .await
                    .expect("Failed to get messages");
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0], b"persistent message");
            }
        }
    }

    #[tokio::test]
    async fn test_get_slice_beyond_available_messages() {
        for factory in create_test_store_factories() {
            let mut store = factory.create_store();

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
    }

    #[tokio::test]
    async fn test_overwrite_existing_message() {
        for factory in create_test_store_factories() {
            let mut store = factory.create_store();

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
}
