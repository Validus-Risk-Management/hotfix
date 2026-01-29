//! MongoDB-specific tests for MongoDbMessageStore.
//!
//! These tests cover MongoDB-specific functionality such as connection failure handling
//! and the cleanup_older_than method.

use chrono::Duration;
use hotfix_store::MessageStore;
use hotfix_store::error::StoreError;
use hotfix_store_mongodb::{Client, MongoDbMessageStore};
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

#[tokio::test]
async fn test_cleanup_removes_old_sequences() {
    let (container, mut store) = create_dedicated_container_and_store().await;

    // Add a message to the initial sequence
    store.add(1, b"message in sequence 1").await.unwrap();

    // Reset creates a new sequence, making the first one "old"
    store.reset().await.unwrap();
    store.add(1, b"message in sequence 2").await.unwrap();

    // Reset again to have two old sequences
    store.reset().await.unwrap();
    store.add(1, b"message in sequence 3").await.unwrap();

    // Small delay to ensure old sequences have earlier timestamps than the cutoff
    tokio::time::sleep(std::time::Duration::from_millis(1)).await;

    // Cleanup with zero duration should delete all old sequences
    let deleted = store.cleanup_older_than(Duration::zero()).await.unwrap();

    assert_eq!(deleted, 2);

    drop(container);
}

#[tokio::test]
async fn test_cleanup_preserves_current_sequence() {
    let (container, mut store) = create_dedicated_container_and_store().await;

    // Add messages to current sequence
    store.add(1, b"message 1").await.unwrap();
    store.add(2, b"message 2").await.unwrap();

    // Cleanup with zero duration - current sequence should be preserved
    let deleted = store.cleanup_older_than(Duration::zero()).await.unwrap();

    assert_eq!(deleted, 0);

    // Verify messages are still accessible
    let messages = store.get_slice(1, 2).await.unwrap();
    assert_eq!(messages.len(), 2);

    drop(container);
}

#[tokio::test]
async fn test_cleanup_respects_age_threshold() {
    let (container, mut store) = create_dedicated_container_and_store().await;

    // Create an old sequence
    store.reset().await.unwrap();

    // Cleanup with a large duration should not delete anything
    let deleted = store.cleanup_older_than(Duration::days(365)).await.unwrap();

    assert_eq!(deleted, 0);

    drop(container);
}

#[tokio::test]
async fn test_cleanup_after_connection_drop() {
    let (container, store) = create_dedicated_container_and_store().await;

    // Stop the container
    container.stop().await.unwrap();

    // Attempt cleanup - should fail
    let result = store.cleanup_older_than(Duration::zero()).await;

    assert!(matches!(result, Err(StoreError::Cleanup(_))));
}
