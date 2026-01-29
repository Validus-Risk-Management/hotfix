//! Conformance tests for InMemoryMessageStore and FileStore implementations.

use std::path::PathBuf;
use std::{env, fs};

use hotfix_store::test_utils::{self, TestStoreFactory};
use hotfix_store::{FileStore, InMemoryMessageStore, MessageStore};

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

struct FileStoreTestFactory {
    directory: PathBuf,
    name: String,
}

impl FileStoreTestFactory {
    fn new() -> Self {
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

macro_rules! conformance_test {
    ($test_name:ident) => {
        #[tokio::test]
        async fn $test_name() {
            test_utils::$test_name(&InMemoryMessageStoreTestFactory).await;
            test_utils::$test_name(&FileStoreTestFactory::new()).await;
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
