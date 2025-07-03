use hotfix::store::in_memory::InMemoryMessageStore;
use hotfix::store::MessageStore;

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
        stores.push(
            Box::new(redb_test_utils::RedbTestStoreFactory::new()) as Box<dyn TestStoreFactory>
        );
    }

    stores
}

struct InMemoryMessageStoreTestFactory;

impl TestStoreFactory for InMemoryMessageStoreTestFactory {
    fn create_store(&self) -> Box<dyn MessageStore> {
        Box::new(InMemoryMessageStore::default())
    }

    fn is_persistent(&self) -> bool {
        false
    }
}

#[cfg(feature = "redb")]
mod redb_test_utils {
    use super::*;
    use hotfix::store::redb::RedbMessageStore;
    use hotfix::store::MessageStore;
    use std::path::PathBuf;
    use std::{env, fs};

    pub(crate) struct RedbTestStoreFactory {
        db_path: PathBuf,
    }

    impl RedbTestStoreFactory {
        pub(crate) fn new() -> Self {
            let mut temp_path = env::temp_dir();
            temp_path.push(format!("redb_test_{}", uuid::Uuid::new_v4()));
            temp_path.set_extension("db");

            Self { db_path: temp_path }
        }
    }

    impl TestStoreFactory for RedbTestStoreFactory {
        fn create_store(&self) -> Box<dyn MessageStore> {
            Box::new(RedbMessageStore::new(&self.db_path).expect("Failed to create store"))
        }
    }

    impl Drop for RedbTestStoreFactory {
        fn drop(&mut self) {
            // Clean up the database file when the test store is dropped
            let _ = fs::remove_file(&self.db_path);
        }
    }
}
