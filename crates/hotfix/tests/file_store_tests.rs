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
