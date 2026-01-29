# hotfix-store

Message store traits and implementations for the [HotFIX](https://github.com/Validus-Risk-Management/hotfix) FIX engine.

## Overview

This crate provides the `MessageStore` trait and core implementations for persisting FIX session state, including
messages and sequence numbers.

## Implementations

- **InMemoryMessageStore**: A non-persistent store for testing
- **FileStore**: A file-based store for simple persistence

Additional implementations are available in separate crates:

- [hotfix-store-mongodb](https://crates.io/crates/hotfix-store-mongodb): MongoDB-backed store

## Usage

```rust
use hotfix_store::{MessageStore, InMemoryMessageStore, FileStore};

// In-memory store (for testing)
let store = InMemoryMessageStore::default ();

// File-based store (for persistence)
let store = FileStore::new("/path/to/store", "session_name") ?;
```

## Test Utilities

The `test-utils` feature provides a test harness for verifying custom `MessageStore` implementations:

```rust
use hotfix_store::test_utils::TestStoreFactory;
use hotfix_store::conformance_tests;

struct MyStoreFactory;

#[async_trait::async_trait]
impl TestStoreFactory for MyStoreFactory {
    async fn create_store(&self) -> Box<dyn MessageStore> {
        Box::new(MyStore::new())
    }
}

// Generates all conformance tests for your implementation
conformance_tests!(my_store, MyStoreFactory);
```
