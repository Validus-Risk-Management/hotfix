# hotfix-store-mongodb

MongoDB message store implementation for the [HotFIX](https://github.com/Validus-Risk-Management/hotfix) FIX engine.

## Overview

This crate provides `MongoDbMessageStore`, a persistent message store backed by MongoDB. It implements the
`MessageStore` trait from [hotfix-store](https://crates.io/crates/hotfix-store).

## Usage

```rust
use hotfix_store_mongodb::{Client, MongoDbMessageStore};

// Connect to MongoDB
let client = Client::with_uri_str("mongodb://localhost:27017").await?;
let db = client.database("myapp");

// Create the store
let store = MongoDbMessageStore::new(db, Some("fix_messages")).await?;
```

## Features

- Persistent storage of FIX messages and sequence numbers
- Automatic index creation for efficient queries
- Session cleanup with `cleanup_older_than()` method

## Cleanup

The store provides a method to clean up old session data:

```rust
use chrono::Duration;

// Delete sequences older than 30 days
let deleted_count = store.cleanup_older_than(Duration::days(30)).await?;
```
