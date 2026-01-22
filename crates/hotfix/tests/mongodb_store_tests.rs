#![cfg(feature = "mongodb")]

use hotfix::store::mongodb::{Client, MongoDbMessageStore};
use hotfix::store::{MessageStore, StoreError};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage};
use tokio::sync::OnceCell;

const MONGO_PORT: u16 = 27017;

static MONGO_CONTAINER: OnceCell<ContainerAsync<GenericImage>> = OnceCell::const_new();

async fn init_container() -> ContainerAsync<GenericImage> {
    GenericImage::new("mongo", "8.0").start().await.unwrap()
}

async fn get_mongo_client() -> Client {
    let container = MONGO_CONTAINER.get_or_init(init_container).await;
    let host = container.get_host().await.unwrap();
    let port = container.get_host_port_ipv4(MONGO_PORT).await.unwrap();
    Client::with_uri_str(format!("mongodb://{host}:{port}"))
        .await
        .unwrap()
}

async fn create_test_store(db_name: &str) -> MongoDbMessageStore {
    let client = get_mongo_client().await;
    let db = client.database(db_name);
    let collection_name = format!("test_{}", uuid::Uuid::new_v4());
    MongoDbMessageStore::new(db, Some(&collection_name))
        .await
        .unwrap()
}

// =============================================================================
// Large Sequence Number Tests
// =============================================================================

mod large_sequence_number_tests {
    use super::*;

    #[tokio::test]
    async fn test_sequence_number_at_u32_max() {
        let mut store = create_test_store("test_seq_u32_max").await;

        let seq_num: u64 = u32::MAX as u64;
        let message = b"message at u32::MAX";

        store.add(seq_num, message).await.unwrap();

        let messages = store
            .get_slice(seq_num as usize, seq_num as usize)
            .await
            .unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], message);
    }

    #[tokio::test]
    async fn test_sequence_number_above_u32_max() {
        let mut store = create_test_store("test_seq_above_u32_max").await;

        let large_seq_num: u64 = u32::MAX as u64 + 100;
        let message = b"message above u32::MAX";

        store.add(large_seq_num, message).await.unwrap();

        let messages = store
            .get_slice(large_seq_num as usize, large_seq_num as usize)
            .await
            .unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], message);
    }

    #[tokio::test]
    async fn test_get_slice_with_large_bounds() {
        let mut store = create_test_store("test_large_bounds").await;

        let base_seq: u64 = u32::MAX as u64 + 1000;
        let messages_to_add = vec![
            (base_seq, b"first".as_slice()),
            (base_seq + 1, b"second".as_slice()),
            (base_seq + 2, b"third".as_slice()),
        ];

        for (seq, msg) in &messages_to_add {
            store.add(*seq, msg).await.unwrap();
        }

        let retrieved = store
            .get_slice(base_seq as usize, (base_seq + 2) as usize)
            .await
            .unwrap();

        assert_eq!(retrieved.len(), 3);
    }

    #[tokio::test]
    async fn test_set_target_seq_number_above_u32_max() {
        let mut store = create_test_store("test_target_seq_large").await;

        let large_seq: u64 = u32::MAX as u64 + 500;

        store.set_target_seq_number(large_seq).await.unwrap();

        assert_eq!(store.next_target_seq_number(), large_seq + 1);

        // Note: We can't easily verify persistence without tracking the collection name,
        // so we just verify the in-memory state was updated correctly
    }

    #[tokio::test]
    async fn test_no_collision_between_large_and_truncated_values() {
        let mut store = create_test_store("test_no_collision").await;

        // These two sequence numbers would collide if truncated to u32
        let small_seq: u64 = 100;
        let large_seq: u64 = u32::MAX as u64 + 100 + 1; // Would truncate to 100 if cast to u32

        let small_msg = b"small sequence message";
        let large_msg = b"large sequence message";

        store.add(small_seq, small_msg).await.unwrap();
        store.add(large_seq, large_msg).await.unwrap();

        // Both messages should be retrievable independently
        let small_retrieved = store
            .get_slice(small_seq as usize, small_seq as usize)
            .await
            .unwrap();
        let large_retrieved = store
            .get_slice(large_seq as usize, large_seq as usize)
            .await
            .unwrap();

        assert_eq!(small_retrieved.len(), 1);
        assert_eq!(small_retrieved[0], small_msg);

        assert_eq!(large_retrieved.len(), 1);
        assert_eq!(large_retrieved[0], large_msg);
    }
}

// =============================================================================
// Concurrent Access Tests
// =============================================================================

mod concurrent_access_tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn test_concurrent_add_same_sequence() {
        let client = get_mongo_client().await;
        let db = client.database("test_concurrent_add");
        let collection_name = format!("test_{}", uuid::Uuid::new_v4());

        let mut store1 = MongoDbMessageStore::new(db.clone(), Some(&collection_name))
            .await
            .unwrap();
        let mut store2 = MongoDbMessageStore::new(db.clone(), Some(&collection_name))
            .await
            .unwrap();

        let seq_num = 1u64;
        let msg1 = b"message from store 1";
        let msg2 = b"message from store 2";

        // Both stores add a message with the same sequence number
        let (result1, result2) = tokio::join!(store1.add(seq_num, msg1), store2.add(seq_num, msg2));

        // Both should succeed (upsert semantics)
        assert!(result1.is_ok());
        assert!(result2.is_ok());

        // One of the messages should be stored (last write wins)
        let messages = store1
            .get_slice(seq_num as usize, seq_num as usize)
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);
        // The message should be one of the two (we can't guarantee which due to race)
        assert!(messages[0] == msg1 || messages[0] == msg2);
    }

    #[tokio::test]
    async fn test_concurrent_increment_sender_seq() {
        let client = get_mongo_client().await;
        let db = client.database("test_concurrent_inc_sender");
        let collection_name = format!("test_{}", uuid::Uuid::new_v4());

        let store = Arc::new(Mutex::new(
            MongoDbMessageStore::new(db.clone(), Some(&collection_name))
                .await
                .unwrap(),
        ));

        let num_increments = 50;
        let mut handles = Vec::new();

        for _ in 0..num_increments {
            let store_clone = Arc::clone(&store);
            handles.push(tokio::spawn(async move {
                let mut guard = store_clone.lock().await;
                guard.increment_sender_seq_number().await
            }));
        }

        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        let final_store = store.lock().await;
        assert_eq!(final_store.next_sender_seq_number(), num_increments + 1);
    }

    #[tokio::test]
    async fn test_concurrent_increment_target_seq() {
        let client = get_mongo_client().await;
        let db = client.database("test_concurrent_inc_target");
        let collection_name = format!("test_{}", uuid::Uuid::new_v4());

        let store = Arc::new(Mutex::new(
            MongoDbMessageStore::new(db.clone(), Some(&collection_name))
                .await
                .unwrap(),
        ));

        let num_increments = 50;
        let mut handles = Vec::new();

        for _ in 0..num_increments {
            let store_clone = Arc::clone(&store);
            handles.push(tokio::spawn(async move {
                let mut guard = store_clone.lock().await;
                guard.increment_target_seq_number().await
            }));
        }

        for handle in handles {
            handle.await.unwrap().unwrap();
        }

        let final_store = store.lock().await;
        assert_eq!(final_store.next_target_seq_number(), num_increments + 1);
    }

    #[tokio::test]
    async fn test_two_store_instances_same_collection() {
        let client = get_mongo_client().await;
        let db = client.database("test_two_instances");
        let collection_name = format!("test_{}", uuid::Uuid::new_v4());

        let mut store1 = MongoDbMessageStore::new(db.clone(), Some(&collection_name))
            .await
            .unwrap();

        // Store1 adds a message
        store1.add(1, b"message from store 1").await.unwrap();
        store1.increment_sender_seq_number().await.unwrap();

        // Store2 connects to the same collection
        let store2 = MongoDbMessageStore::new(db.clone(), Some(&collection_name))
            .await
            .unwrap();

        // Store2 should see the updated sequence number
        assert_eq!(store2.next_sender_seq_number(), 2);

        // Store2 should be able to retrieve the message
        let messages = store2.get_slice(1, 1).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], b"message from store 1");
    }
}

// =============================================================================
// Connection Failure Tests
// =============================================================================

mod connection_failure_tests {
    use super::*;

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
}

// =============================================================================
// Data Integrity Tests
// =============================================================================

mod data_integrity_tests {
    use super::*;

    #[tokio::test]
    async fn test_store_empty_message() {
        let mut store = create_test_store("test_empty_msg").await;

        let empty_message: &[u8] = b"";

        store.add(1, empty_message).await.unwrap();

        let messages = store.get_slice(1, 1).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], empty_message);
    }

    #[tokio::test]
    async fn test_store_binary_message() {
        let mut store = create_test_store("test_binary_msg").await;

        // Message with null bytes and other binary data
        let binary_message: Vec<u8> = (0u8..=255).collect();

        store.add(1, &binary_message).await.unwrap();

        let messages = store.get_slice(1, 1).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], binary_message);
    }

    #[tokio::test]
    async fn test_store_large_message() {
        let mut store = create_test_store("test_large_msg").await;

        // 1MB message (well under 16MB BSON limit)
        let large_message: Vec<u8> = vec![0xAB; 1024 * 1024];

        store.add(1, &large_message).await.unwrap();

        let messages = store.get_slice(1, 1).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], large_message);
    }

    #[tokio::test]
    async fn test_sequence_number_zero() {
        let mut store = create_test_store("test_seq_zero").await;

        store.add(0, b"message at seq 0").await.unwrap();

        let messages = store.get_slice(0, 0).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], b"message at seq 0");
    }

    #[tokio::test]
    async fn test_overwrite_message() {
        let mut store = create_test_store("test_overwrite").await;

        store.add(1, b"original message").await.unwrap();
        store.add(1, b"updated message").await.unwrap();

        let messages = store.get_slice(1, 1).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], b"updated message");
    }

    #[tokio::test]
    async fn test_sparse_sequence_numbers() {
        let mut store = create_test_store("test_sparse").await;

        // Add messages with gaps in sequence numbers
        store.add(1, b"first").await.unwrap();
        store.add(100, b"hundredth").await.unwrap();
        store.add(1000, b"thousandth").await.unwrap();

        // Query the full range - should only return existing messages
        let messages = store.get_slice(1, 1000).await.unwrap();
        assert_eq!(messages.len(), 3);
    }

    #[tokio::test]
    async fn test_reset_creates_new_sequence() {
        let mut store = create_test_store("test_reset_new_seq").await;

        // Add data and increment sequence numbers
        store.add(1, b"pre-reset message").await.unwrap();
        store.increment_sender_seq_number().await.unwrap();
        store.increment_target_seq_number().await.unwrap();

        let creation_time_before = store.creation_time();

        // Small delay to ensure different creation time
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Reset
        store.reset().await.unwrap();

        // Sequence numbers should be reset
        assert_eq!(store.next_sender_seq_number(), 1);
        assert_eq!(store.next_target_seq_number(), 1);

        // Creation time should be updated
        assert!(store.creation_time() > creation_time_before);
    }
}
