//! Conformance tests for MongoDbMessageStore using the test harness from hotfix-store.

use hotfix_store::test_utils::{self, TestStoreFactory};
use hotfix_store::MessageStore;
use hotfix_store_mongodb::{Client, MongoDbMessageStore};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage};
use tokio::sync::OnceCell;

static MONGO_CONTAINER: OnceCell<ContainerAsync<GenericImage>> = OnceCell::const_new();
const MONGO_PORT: u16 = 27017;

struct MongodbTestStoreFactory {
    client: Client,
    collection_name: String,
}

impl MongodbTestStoreFactory {
    async fn new() -> Self {
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
        let db = self.client.database("hotfixConformanceTests");
        let store = MongoDbMessageStore::new(db, Some(&self.collection_name))
            .await
            .unwrap();
        Box::new(store)
    }
}

// Each test gets its own factory with a unique collection name to ensure isolation
macro_rules! conformance_test {
    ($test_name:ident) => {
        #[tokio::test]
        async fn $test_name() {
            let factory = MongodbTestStoreFactory::new().await;
            test_utils::$test_name(&factory).await;
        }
    };
}

conformance_test!(test_new_store_initialization);
conformance_test!(test_add_and_get_messages);
conformance_test!(test_get_slice_partial_range);
conformance_test!(test_get_slice_empty_range);
conformance_test!(test_increment_sender_seq_number);
conformance_test!(test_increment_target_seq_number);
conformance_test!(test_set_target_seq_number);
conformance_test!(test_reset_store);
conformance_test!(test_persistence_across_store_instances);
conformance_test!(test_get_slice_beyond_available_messages);
conformance_test!(test_overwrite_existing_message);
conformance_test!(test_creation_time_is_set);
conformance_test!(test_creation_time_is_preserved);
conformance_test!(test_creation_time_gets_reset_correctly);
