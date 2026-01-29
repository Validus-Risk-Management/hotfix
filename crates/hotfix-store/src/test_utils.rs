//! Test utilities for message store implementations.
//!
//! This module provides a test harness for verifying that message store
//! implementations conform to the expected behavior of the [`MessageStore`] trait.

use crate::MessageStore;

/// A factory trait for creating message store instances during testing.
///
/// Implement this trait for each message store implementation you want to test.
/// The test functions in this module will use this factory to create fresh
/// store instances for each test.
#[async_trait::async_trait]
pub trait TestStoreFactory: Send + Sync {
    /// Creates a new message store instance.
    ///
    /// For persistent stores, this should return a store that can be
    /// reopened with the same data (by calling `create_store` again).
    async fn create_store(&self) -> Box<dyn MessageStore>;

    /// Returns `true` if the store persists data across restarts.
    ///
    /// This is used to skip persistence-related tests for in-memory stores.
    fn is_persistent(&self) -> bool {
        true
    }
}

/// Tests that a new store starts with sequence numbers at 1.
pub async fn test_new_store_initialization(factory: &dyn TestStoreFactory) {
    let store = factory.create_store().await;

    assert_eq!(store.next_sender_seq_number(), 1);
    assert_eq!(store.next_target_seq_number(), 1);
}

/// Tests adding and retrieving messages.
pub async fn test_add_and_get_messages(factory: &dyn TestStoreFactory) {
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

/// Tests getting a partial range of messages.
pub async fn test_get_slice_partial_range(factory: &dyn TestStoreFactory) {
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

/// Tests getting a slice from an empty store.
pub async fn test_get_slice_empty_range(factory: &dyn TestStoreFactory) {
    let store = factory.create_store().await;

    let messages = store.get_slice(1, 3).await.expect("Failed to get messages");
    assert_eq!(messages.len(), 0);
}

/// Tests incrementing the sender sequence number.
pub async fn test_increment_sender_seq_number(factory: &dyn TestStoreFactory) {
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

/// Tests incrementing the target sequence number.
pub async fn test_increment_target_seq_number(factory: &dyn TestStoreFactory) {
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

/// Tests setting the target sequence number directly.
pub async fn test_set_target_seq_number(factory: &dyn TestStoreFactory) {
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

/// Tests resetting the store.
pub async fn test_reset_store(factory: &dyn TestStoreFactory) {
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

/// Tests that data persists across store instances (for persistent stores only).
pub async fn test_persistence_across_store_instances(factory: &dyn TestStoreFactory) {
    if !factory.is_persistent() {
        return;
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

/// Tests getting a slice beyond available messages.
pub async fn test_get_slice_beyond_available_messages(factory: &dyn TestStoreFactory) {
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

/// Tests overwriting an existing message.
pub async fn test_overwrite_existing_message(factory: &dyn TestStoreFactory) {
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

/// Tests that creation time is set on new stores.
pub async fn test_creation_time_is_set(factory: &dyn TestStoreFactory) {
    use chrono::Utc;

    let before = Utc::now();
    let store = factory.create_store().await;
    let after = Utc::now();

    assert!(before <= store.creation_time());
    assert!(store.creation_time() <= after);
}

/// Tests that creation time is preserved across store restarts (for persistent stores only).
pub async fn test_creation_time_is_preserved(factory: &dyn TestStoreFactory) {
    if !factory.is_persistent() {
        return;
    }

    let store = factory.create_store().await;
    let creation_time1 = store.creation_time();
    drop(store);

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let store = factory.create_store().await;
    let creation_time2 = store.creation_time();

    assert_eq!(creation_time1, creation_time2);
}

/// Tests that creation time is updated on reset.
pub async fn test_creation_time_gets_reset_correctly(factory: &dyn TestStoreFactory) {
    use chrono::Utc;

    let mut store = factory.create_store().await;

    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let after_sleep = Utc::now();
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;

    store.reset().await.expect("failed to reset store");
    let reset_creation_time = store.creation_time();
    assert!(reset_creation_time > after_sleep);

    if !factory.is_persistent() {
        return;
    }

    drop(store);
    let store = factory.create_store().await;
    assert_eq!(reset_creation_time, store.creation_time());
}

/// Generates conformance tests for a message store implementation.
///
/// This macro creates a module containing all the standard conformance tests
/// for a [`MessageStore`] implementation. Each test gets its own test function,
/// allowing for parallel execution and clear test reporting.
///
/// # Arguments
///
/// * `$mod_name` - The name of the module to create (e.g., `in_memory`, `file`, `mongodb`)
/// * `$factory` - An expression that creates a [`TestStoreFactory`] instance.
///   This can be a sync expression (e.g., `MyFactory::new()`) or an async
///   expression (e.g., `MyFactory::new().await`).
///
/// # Example
///
/// ```ignore
/// use hotfix_store::conformance_tests;
///
/// struct MyStoreFactory;
///
/// #[async_trait::async_trait]
/// impl TestStoreFactory for MyStoreFactory {
///     async fn create_store(&self) -> Box<dyn MessageStore> {
///         Box::new(MyStore::new())
///     }
/// }
///
/// conformance_tests!(my_store, MyStoreFactory);
/// ```
#[macro_export]
macro_rules! conformance_tests {
    ($mod_name:ident, $factory:expr) => {
        mod $mod_name {
            use super::*;

            #[tokio::test]
            async fn test_new_store_initialization() {
                let factory = $factory;
                $crate::test_utils::test_new_store_initialization(&factory).await;
            }

            #[tokio::test]
            async fn test_add_and_get_messages() {
                let factory = $factory;
                $crate::test_utils::test_add_and_get_messages(&factory).await;
            }

            #[tokio::test]
            async fn test_get_slice_partial_range() {
                let factory = $factory;
                $crate::test_utils::test_get_slice_partial_range(&factory).await;
            }

            #[tokio::test]
            async fn test_get_slice_empty_range() {
                let factory = $factory;
                $crate::test_utils::test_get_slice_empty_range(&factory).await;
            }

            #[tokio::test]
            async fn test_increment_sender_seq_number() {
                let factory = $factory;
                $crate::test_utils::test_increment_sender_seq_number(&factory).await;
            }

            #[tokio::test]
            async fn test_increment_target_seq_number() {
                let factory = $factory;
                $crate::test_utils::test_increment_target_seq_number(&factory).await;
            }

            #[tokio::test]
            async fn test_set_target_seq_number() {
                let factory = $factory;
                $crate::test_utils::test_set_target_seq_number(&factory).await;
            }

            #[tokio::test]
            async fn test_reset_store() {
                let factory = $factory;
                $crate::test_utils::test_reset_store(&factory).await;
            }

            #[tokio::test]
            async fn test_persistence_across_store_instances() {
                let factory = $factory;
                $crate::test_utils::test_persistence_across_store_instances(&factory).await;
            }

            #[tokio::test]
            async fn test_get_slice_beyond_available_messages() {
                let factory = $factory;
                $crate::test_utils::test_get_slice_beyond_available_messages(&factory).await;
            }

            #[tokio::test]
            async fn test_overwrite_existing_message() {
                let factory = $factory;
                $crate::test_utils::test_overwrite_existing_message(&factory).await;
            }

            #[tokio::test]
            async fn test_creation_time_is_set() {
                let factory = $factory;
                $crate::test_utils::test_creation_time_is_set(&factory).await;
            }

            #[tokio::test]
            async fn test_creation_time_is_preserved() {
                let factory = $factory;
                $crate::test_utils::test_creation_time_is_preserved(&factory).await;
            }

            #[tokio::test]
            async fn test_creation_time_gets_reset_correctly() {
                let factory = $factory;
                $crate::test_utils::test_creation_time_gets_reset_correctly(&factory).await;
            }
        }
    };
}
