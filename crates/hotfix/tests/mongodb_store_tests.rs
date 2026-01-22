#![cfg(feature = "mongodb")]

use hotfix::store::mongodb::{Client, MongoDbMessageStore};
use hotfix::store::{MessageStore, StoreError};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage};

const MONGO_PORT: u16 = 27017;

async fn create_dedicated_container_and_store()
-> (ContainerAsync<GenericImage>, MongoDbMessageStore) {
    let container = GenericImage::new("mongo", "8.0").start().await.unwrap();
    let host = container.get_host().await.unwrap();
    let port = container.get_host_port_ipv4(MONGO_PORT).await.unwrap();

    let client = Client::with_uri_str(format!("mongodb://{host}:{port}"))
        .await
        .unwrap();
    let db = client.database("test_conn_failure");
    let store = MongoDbMessageStore::new(db, Some("test")).await.unwrap();

    (container, store)
}

#[tokio::test]
async fn test_add_after_connection_drop() {
    let (container, mut store) = create_dedicated_container_and_store().await;

    // Verify store works initially
    store.add(1, b"initial message").await.unwrap();

    // Stop the container
    container.stop().await.unwrap();

    // Attempt operation - should fail with appropriate error
    let result = store.add(2, b"should fail").await;

    assert!(matches!(result, Err(StoreError::PersistMessage { .. })));
}

#[tokio::test]
async fn test_get_slice_after_connection_drop() {
    let (container, mut store) = create_dedicated_container_and_store().await;

    // Add a message while connected
    store.add(1, b"test message").await.unwrap();

    // Stop the container
    container.stop().await.unwrap();

    // Attempt retrieval - should fail
    let result = store.get_slice(1, 1).await;

    assert!(matches!(result, Err(StoreError::RetrieveMessages { .. })));
}

#[tokio::test]
async fn test_increment_after_connection_drop() {
    let (container, mut store) = create_dedicated_container_and_store().await;

    // Stop the container
    container.stop().await.unwrap();

    // Attempt increment - should fail
    let result = store.increment_sender_seq_number().await;

    assert!(matches!(result, Err(StoreError::UpdateSequenceNumber(_))));
}

#[tokio::test]
async fn test_reset_after_connection_drop() {
    let (container, mut store) = create_dedicated_container_and_store().await;

    // Stop the container
    container.stop().await.unwrap();

    // Attempt reset - should fail
    let result = store.reset().await;

    assert!(matches!(result, Err(StoreError::Reset(_))));
}

#[tokio::test]
async fn test_state_preserved_after_failed_increment() {
    let (container, mut store) = create_dedicated_container_and_store().await;

    let initial_sender_seq = store.next_sender_seq_number();
    let initial_target_seq = store.next_target_seq_number();

    // Stop the container
    container.stop().await.unwrap();

    // Attempt increments - should fail
    let _ = store.increment_sender_seq_number().await;
    let _ = store.increment_target_seq_number().await;

    // State should be unchanged since DB write failed first
    assert_eq!(store.next_sender_seq_number(), initial_sender_seq);
    assert_eq!(store.next_target_seq_number(), initial_target_seq);
}

#[tokio::test]
async fn test_state_preserved_after_failed_set_target() {
    let (container, mut store) = create_dedicated_container_and_store().await;

    let initial_target_seq = store.next_target_seq_number();

    // Stop the container
    container.stop().await.unwrap();

    // Attempt set - should fail
    let _ = store.set_target_seq_number(100).await;

    // State should be unchanged
    assert_eq!(store.next_target_seq_number(), initial_target_seq);
}
