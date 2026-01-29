//! Conformance tests for InMemoryMessageStore and FileStore implementations.

use std::path::PathBuf;
use std::{env, fs};

use hotfix_store::test_utils::TestStoreFactory;
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

hotfix_store::conformance_tests!(in_memory, InMemoryMessageStoreTestFactory);
hotfix_store::conformance_tests!(file, FileStoreTestFactory::new());
