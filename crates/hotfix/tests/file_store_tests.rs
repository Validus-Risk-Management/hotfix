use hotfix::store::file::FileStore;
use hotfix::store::{MessageStore, StoreError};
use std::fs;
use tempfile::TempDir;

fn create_test_store() -> (TempDir, FileStore) {
    let dir = TempDir::new().unwrap();
    let store = FileStore::new(dir.path(), "test").unwrap();
    (dir, store)
}

mod corrupted_file_tests {
    use super::*;

    #[tokio::test]
    async fn test_corrupted_seqnums_file() {
        let dir = TempDir::new().unwrap();
        let base_path = dir.path().join("test");

        // Create required session file
        fs::write(
            base_path.with_extension("session"),
            chrono::Utc::now().to_rfc3339(),
        )
        .unwrap();

        // Create a corrupted seqnums file (invalid format)
        fs::write(base_path.with_extension("seqnums"), "not:valid:format").unwrap();

        let result = FileStore::new(dir.path(), "test");

        assert!(result.is_err());
        let err = result.err().unwrap();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("seqnums") || err_msg.contains("parse"),
            "Error should mention seqnums parsing: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn test_corrupted_session_file() {
        let dir = TempDir::new().unwrap();
        let base_path = dir.path().join("test");

        // Create a corrupted session file (invalid datetime)
        fs::write(base_path.with_extension("session"), "not-a-valid-datetime").unwrap();

        let result = FileStore::new(dir.path(), "test");

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_corrupted_header_file() {
        let dir = TempDir::new().unwrap();
        let base_path = dir.path().join("test");

        // Create valid session file
        fs::write(
            base_path.with_extension("session"),
            chrono::Utc::now().to_rfc3339(),
        )
        .unwrap();

        // Create body file with some content
        fs::write(base_path.with_extension("body"), b"test message content").unwrap();

        // Create corrupted header file (malformed lines)
        // FileStore silently skips malformed lines, so this should succeed
        // but the message won't be found
        fs::write(
            base_path.with_extension("header"),
            "invalid_line_without_commas\n1,not_a_number,10\n",
        )
        .unwrap();

        let store = FileStore::new(dir.path(), "test");

        // Store creation should succeed (malformed lines are skipped)
        assert!(store.is_ok());

        let store = store.unwrap();
        // No messages should be loaded due to malformed headers
        let messages = store.get_slice(1, 10).await.unwrap();
        assert_eq!(messages.len(), 0);
    }

    #[tokio::test]
    async fn test_partial_seqnums_content() {
        let dir = TempDir::new().unwrap();
        let base_path = dir.path().join("test");

        // Create required session file
        fs::write(
            base_path.with_extension("session"),
            chrono::Utc::now().to_rfc3339(),
        )
        .unwrap();

        // Create truncated seqnums file (only one number)
        fs::write(base_path.with_extension("seqnums"), "00000000000000000001").unwrap();

        let result = FileStore::new(dir.path(), "test");

        assert!(result.is_err());
        let err = result.err().unwrap();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("seqnums") || err_msg.contains("format"),
            "Error should mention format issue: {}",
            err_msg
        );
    }
}

mod file_system_error_tests {
    use super::*;

    #[tokio::test]
    #[cfg(unix)]
    async fn test_readonly_directory() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let readonly_dir = dir.path().join("readonly");
        fs::create_dir(&readonly_dir).unwrap();

        // Make directory read-only
        let mut perms = fs::metadata(&readonly_dir).unwrap().permissions();
        perms.set_mode(0o444);
        fs::set_permissions(&readonly_dir, perms).unwrap();

        let result = FileStore::new(&readonly_dir, "test");

        // Should fail because we can't create files
        assert!(result.is_err());

        // Restore permissions for cleanup
        let mut perms = fs::metadata(&readonly_dir).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&readonly_dir, perms).unwrap();
    }

    #[tokio::test]
    async fn test_missing_body_file_on_read() {
        let (dir, mut store) = create_test_store();

        // Add a message
        store.add(1, b"test message").await.unwrap();

        // Delete the body file
        let body_path = dir.path().join("test.body");
        fs::remove_file(&body_path).unwrap();

        // Attempt to read - should fail
        let result = store.get_slice(1, 1).await;

        assert!(matches!(result, Err(StoreError::RetrieveMessages { .. })));
    }

    #[tokio::test]
    async fn test_directory_not_exists_creates_it() {
        let dir = TempDir::new().unwrap();
        let nested_path = dir
            .path()
            .join("nested")
            .join("path")
            .join("to")
            .join("store");

        // Directory doesn't exist yet
        assert!(!nested_path.exists());

        // FileStore should create the directory
        let result = FileStore::new(&nested_path, "test");
        assert!(result.is_ok());

        // Directory should now exist
        assert!(nested_path.exists());
    }

    #[tokio::test]
    async fn test_body_file_truncated() {
        let (dir, mut store) = create_test_store();

        // Add a message
        store.add(1, b"test message content").await.unwrap();

        // Truncate the body file
        let body_path = dir.path().join("test.body");
        fs::write(&body_path, b"short").unwrap();

        // Attempt to read - should fail because we can't read expected bytes
        let result = store.get_slice(1, 1).await;

        assert!(matches!(result, Err(StoreError::RetrieveMessages { .. })));
    }
}

mod data_integrity_tests {
    use super::*;

    #[tokio::test]
    async fn test_store_empty_message() {
        let (_dir, mut store) = create_test_store();

        let empty_message: &[u8] = b"";

        store.add(1, empty_message).await.unwrap();

        let messages = store.get_slice(1, 1).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], empty_message);
    }

    #[tokio::test]
    async fn test_store_binary_message() {
        let (_dir, mut store) = create_test_store();

        // Message with null bytes and all possible byte values
        let binary_message: Vec<u8> = (0u8..=255).collect();

        store.add(1, &binary_message).await.unwrap();

        let messages = store.get_slice(1, 1).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], binary_message);
    }

    #[tokio::test]
    async fn test_store_large_message() {
        let (_dir, mut store) = create_test_store();

        // 1MB message
        let large_message: Vec<u8> = vec![0xAB; 1024 * 1024];

        store.add(1, &large_message).await.unwrap();

        let messages = store.get_slice(1, 1).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], large_message);
    }

    #[tokio::test]
    async fn test_sparse_sequence_numbers() {
        let (_dir, mut store) = create_test_store();

        // Add messages with gaps in sequence numbers
        store.add(1, b"first").await.unwrap();
        store.add(100, b"hundredth").await.unwrap();
        store.add(1000, b"thousandth").await.unwrap();

        // Query the full range - should only return existing messages
        let messages = store.get_slice(1, 1000).await.unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0], b"first");
        assert_eq!(messages[1], b"hundredth");
        assert_eq!(messages[2], b"thousandth");
    }
}

mod concurrent_access_tests {
    use super::*;

    #[tokio::test]
    async fn test_two_store_instances_same_files() {
        let dir = TempDir::new().unwrap();

        // Create first store and add data
        let mut store1 = FileStore::new(dir.path(), "test").unwrap();
        store1.add(1, b"message from store 1").await.unwrap();
        store1.increment_sender_seq_number().await.unwrap();

        // Create second store pointing to same files
        // Note: FileStore reads state at construction time, so it should see
        // the current persisted state
        let store2 = FileStore::new(dir.path(), "test").unwrap();

        // Store2 should see the persisted sequence number
        assert_eq!(store2.next_sender_seq_number(), 2);

        // Store2 should be able to read the message
        let messages = store2.get_slice(1, 1).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], b"message from store 1");
    }

    #[tokio::test]
    async fn test_store_instance_isolation() {
        let dir = TempDir::new().unwrap();

        // Create two store instances
        let mut store1 = FileStore::new(dir.path(), "test").unwrap();
        let mut store2 = FileStore::new(dir.path(), "test").unwrap();

        // Both start at the same sequence number
        assert_eq!(store1.next_sender_seq_number(), 1);
        assert_eq!(store2.next_sender_seq_number(), 1);

        // Store1 increments
        store1.increment_sender_seq_number().await.unwrap();

        // Store1 sees the change
        assert_eq!(store1.next_sender_seq_number(), 2);

        // Store2 still has its cached state (no automatic refresh)
        assert_eq!(store2.next_sender_seq_number(), 1);

        // However, if store2 also writes, it will overwrite with its state
        store2.increment_sender_seq_number().await.unwrap();
        assert_eq!(store2.next_sender_seq_number(), 2);

        // Now if we create a new store, it reads what was last written (store2's state)
        let store3 = FileStore::new(dir.path(), "test").unwrap();
        assert_eq!(store3.next_sender_seq_number(), 2);
    }

    #[tokio::test]
    async fn test_reset_affects_all_future_instances() {
        let dir = TempDir::new().unwrap();

        // Create store and add data
        let mut store1 = FileStore::new(dir.path(), "test").unwrap();
        store1.add(1, b"will be deleted").await.unwrap();
        store1.increment_sender_seq_number().await.unwrap();
        store1.increment_sender_seq_number().await.unwrap();

        // Reset
        store1.reset().await.unwrap();

        // New instance should see reset state
        let store2 = FileStore::new(dir.path(), "test").unwrap();
        assert_eq!(store2.next_sender_seq_number(), 1);
        assert_eq!(store2.next_target_seq_number(), 1);

        let messages = store2.get_slice(1, 100).await.unwrap();
        assert_eq!(messages.len(), 0);
    }
}
