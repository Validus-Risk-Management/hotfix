//! Conformance tests for MongoDbMessageStore using the test harness from hotfix-store.

use hotfix_store::MessageStore;
use hotfix_store::test_utils::TestStoreFactory;
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

hotfix_store::conformance_tests!(mongodb, MongodbTestStoreFactory::new().await);
