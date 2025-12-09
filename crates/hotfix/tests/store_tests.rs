use std::path::PathBuf;
use std::{env, fs};

use chrono::Utc;
use hotfix::store::MessageStore;
use hotfix::store::file::FileStore;
use hotfix::store::in_memory::InMemoryMessageStore;

#[tokio::test]
async fn test_new_store_initialization() {
    for factory in create_test_store_factories().await {
        let store = factory.create_store().await;

        assert_eq!(store.next_sender_seq_number(), 1);
        assert_eq!(store.next_target_seq_number(), 1);
    }
}

#[tokio::test]
async fn test_add_and_get_messages() {
    for factory in create_test_store_factories().await {
        let mut store = factory.create_store().await;

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
    for factory in create_test_store_factories().await {
        let mut store = factory.create_store().await;

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
    for factory in create_test_store_factories().await {
        let store = factory.create_store().await;

        let messages = store.get_slice(1, 3).await.expect("Failed to get messages");
        assert_eq!(messages.len(), 0);
    }
}

#[tokio::test]
async fn test_increment_sender_seq_number() {
    for factory in create_test_store_factories().await {
        let mut store = factory.create_store().await;

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
    for factory in create_test_store_factories().await {
        let mut store = factory.create_store().await;

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
    for factory in create_test_store_factories().await {
        let mut store = factory.create_store().await;

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
    for factory in create_test_store_factories().await {
        let mut store = factory.create_store().await;

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
    for factory in create_test_store_factories().await {
        if !factory.is_persistent() {
            continue;
        }

        // Create first store instance and add data
        {
            let mut store1 = factory.create_store().await;
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
            let store2 = factory.create_store().await;

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
    for factory in create_test_store_factories().await {
        let mut store = factory.create_store().await;

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
    for factory in create_test_store_factories().await {
        let mut store = factory.create_store().await;

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

#[tokio::test]
async fn test_creation_time_is_set() {
    for factory in create_test_store_factories().await {
        let before = Utc::now();
        let store = factory.create_store().await;
        let after = Utc::now();

        assert!(before <= store.creation_time());
        assert!(store.creation_time() <= after);
    }
}

#[tokio::test]
async fn test_creation_time_is_preserved() {
    for factory in create_test_store_factories().await {
        if !factory.is_persistent() {
            continue;
        }

        let store = factory.create_store().await;
        let creation_time1 = store.creation_time();
        drop(store);

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let store = factory.create_store().await;
        let creation_time2 = store.creation_time();

        assert_eq!(creation_time1, creation_time2);
    }
}

#[tokio::test]
async fn test_creation_time_gets_reset_correctly() {
    for factory in create_test_store_factories().await {
        let mut store = factory.create_store().await;

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let after_sleep = Utc::now();

        store.reset().await.expect("failed to reset store");
        let reset_creation_time = store.creation_time();
        assert!(reset_creation_time >= after_sleep);

        if !factory.is_persistent() {
            continue;
        }

        drop(store);
        let store = factory.create_store().await;
        assert_eq!(reset_creation_time, store.creation_time());
    }
}

#[async_trait::async_trait]
pub trait TestStoreFactory {
    async fn create_store(&self) -> Box<dyn MessageStore>;
    fn is_persistent(&self) -> bool {
        true
    }
}

async fn create_test_store_factories() -> Vec<Box<dyn TestStoreFactory>> {
    #[allow(unused_mut)]
    let mut stores: Vec<Box<dyn TestStoreFactory>> = vec![
        // Add in-memory store factory
        Box::new(InMemoryMessageStoreTestFactory {}) as Box<dyn TestStoreFactory>,
        // Add file store factory
        Box::new(FileStoreTestFactory::new()) as Box<dyn TestStoreFactory>,
    ];

    // Add redb store factory if the feature is enabled
    #[cfg(feature = "redb")]
    {
        stores.push(
            Box::new(redb_test_utils::RedbTestStoreFactory::new()) as Box<dyn TestStoreFactory>
        );
    }

    #[cfg(feature = "mongodb")]
    {
        stores.push(
            Box::new(mongodb_test_utils::MongodbTestStoreFactory::new().await)
                as Box<dyn TestStoreFactory>,
        );
    }

    stores
}

struct InMemoryMessageStoreTestFactory;

#[async_trait::async_trait]
impl TestStoreFactory for InMemoryMessageStoreTestFactory {
    async fn create_store(&self) -> Box<dyn MessageStore> {
        Box::new(InMemoryMessageStore::default())
    }

    fn is_persistent(&self) -> bool {
        false
    }
}

pub(crate) struct FileStoreTestFactory {
    directory: PathBuf,
    name: String,
}

impl FileStoreTestFactory {
    pub(crate) fn new() -> Self {
        Self {
            directory: env::temp_dir(),
            name: format!("file_store_test_{}", uuid::Uuid::new_v4()),
        }
    }
}

#[async_trait::async_trait]
impl TestStoreFactory for FileStoreTestFactory {
    async fn create_store(&self) -> Box<dyn MessageStore> {
        Box::new(FileStore::new(&self.directory, &self.name).expect("Failed to create file store"))
    }
}

impl Drop for FileStoreTestFactory {
    fn drop(&mut self) {
        let base_path = self.directory.join(&self.name);
        for ext in ["header", "body", "seqnums", "session"] {
            let _ = fs::remove_file(base_path.with_extension(ext));
        }
    }
}

#[cfg(feature = "redb")]
mod redb_test_utils {
    use super::*;
    use hotfix::store::MessageStore;
    use hotfix::store::redb::RedbMessageStore;
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

    #[async_trait::async_trait]
    impl TestStoreFactory for RedbTestStoreFactory {
        async fn create_store(&self) -> Box<dyn MessageStore> {
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

#[cfg(feature = "mongodb")]
mod mongodb_test_utils {
    use crate::TestStoreFactory;
    use hotfix::store::MessageStore;
    use hotfix::store::mongodb::MongoDbMessageStore;
    use mongodb::Client;
    use testcontainers::runners::AsyncRunner;
    use testcontainers::{ContainerAsync, GenericImage};
    use tokio::sync::OnceCell;

    static MONGO_CONTAINER: OnceCell<ContainerAsync<GenericImage>> = OnceCell::const_new();
    const MONGO_PORT: u16 = 27017;

    pub(crate) struct MongodbTestStoreFactory {
        client: Client,
        collection_name: String,
    }

    impl MongodbTestStoreFactory {
        pub(crate) async fn new() -> Self {
            let container = MONGO_CONTAINER.get_or_init(Self::init_container).await;
            let host = container.get_host().await.unwrap();
            let port = container.get_host_port_ipv4(MONGO_PORT).await.unwrap();
            let uri = format!("mongodb://{host}:{port}");
            let client = Client::with_uri_str(&uri).await.unwrap();

            Self {
                client,
                collection_name: uuid::Uuid::new_v4().to_string(),
            }
        }

        async fn init_container() -> ContainerAsync<GenericImage> {
            GenericImage::new("mongo", "8.0").start().await.unwrap()
        }
    }

    #[async_trait::async_trait]
    impl TestStoreFactory for MongodbTestStoreFactory {
        async fn create_store(&self) -> Box<dyn MessageStore> {
            let db = self.client.database("hotfixIntegrationTests");
            let store = MongoDbMessageStore::new(db, Some(&self.collection_name))
                .await
                .unwrap();
            Box::new(store)
        }
    }
}
