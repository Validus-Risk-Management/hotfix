//! MongoDB message store implementation for the hotfix FIX engine.
//!
//! This crate provides [`MongoDbMessageStore`], a persistent message store
//! backed by MongoDB.
//!
//! # Example
//!
//! ```ignore
//! use hotfix_store_mongodb::{Client, MongoDbMessageStore};
//!
//! let client = Client::with_uri_str("mongodb://localhost:27017").await?;
//! let db = client.database("myapp");
//! let store = MongoDbMessageStore::new(db, Some("fix_messages")).await?;
//! ```

use async_trait::async_trait;
use chrono::{DateTime, Duration, TimeZone, Utc};
use futures::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::spec::BinarySubtype;
use mongodb::bson::{Binary, DateTime as BsonDateTime};
use mongodb::options::{FindOneOptions, IndexOptions, ReplaceOptions};
use mongodb::{Collection, Database, IndexModel};
use serde::{Deserialize, Serialize};

use hotfix_store::MessageStore;
use hotfix_store::error::{Result, StoreError};

pub use mongodb::Client;

const DEFAULT_COLLECTION_NAME: &str = "messages";

#[derive(Debug, Deserialize, Serialize)]
struct SequenceMeta {
    #[serde(rename = "_id")]
    object_id: ObjectId,
    meta: bool,
    creation_time: BsonDateTime,
    sender_seq_number: u64,
    target_seq_number: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct Message {
    sequence_id: ObjectId,
    msg_seq_number: u64,
    data: Binary,
}

/// A MongoDB-backed message store implementation.
///
/// This store persists messages and sequence numbers to MongoDB,
/// allowing session state to survive application restarts.
pub struct MongoDbMessageStore {
    meta_collection: Collection<SequenceMeta>,
    message_collection: Collection<Message>,
    current_sequence: SequenceMeta,
}

impl MongoDbMessageStore {
    /// Creates a new MongoDB message store.
    ///
    /// # Arguments
    ///
    /// * `db` - The MongoDB database to use
    /// * `collection_name` - Optional collection name (defaults to "messages")
    ///
    /// # Errors
    ///
    /// Returns `StoreError::Initialization` if the store cannot be initialized.
    pub async fn new(db: Database, collection_name: Option<&str>) -> Result<Self> {
        Self::new_inner(db, collection_name)
            .await
            .map_err(|e| StoreError::Initialization(e.into()))
    }

    async fn new_inner(
        db: Database,
        collection_name: Option<&str>,
    ) -> mongodb::error::Result<Self> {
        let collection_name = collection_name.unwrap_or(DEFAULT_COLLECTION_NAME);
        let meta_collection = db.collection(collection_name);
        let message_collection = db.collection(collection_name);

        let current_sequence = Self::get_or_default_sequence(&meta_collection).await?;
        Self::ensure_indexes(&meta_collection).await?;

        let store = Self {
            meta_collection,
            message_collection,
            current_sequence,
        };
        Ok(store)
    }

    async fn ensure_indexes(
        meta_collection: &Collection<SequenceMeta>,
    ) -> mongodb::error::Result<()> {
        let meta_index = IndexModel::builder()
            .keys(doc! { "meta": 1, "_id": -1 })
            .build();
        let message_index_options = IndexOptions::builder().unique(true).sparse(true).build();
        let message_index = IndexModel::builder()
            .keys(doc! { "sequence_id": 1, "msg_seq_number": 1})
            .options(Some(message_index_options))
            .build();

        meta_collection
            .create_indexes(vec![meta_index, message_index])
            .await?;
        Ok(())
    }

    async fn get_or_default_sequence(
        meta_collection: &Collection<SequenceMeta>,
    ) -> mongodb::error::Result<SequenceMeta> {
        let options = FindOneOptions::builder().sort(doc! { "_id": -1 }).build();
        let res = meta_collection
            .find_one(doc! { "meta": true })
            .with_options(options)
            .await?;

        let meta = match res {
            None => Self::new_sequence(meta_collection).await?,
            Some(meta) => meta,
        };
        Ok(meta)
    }

    async fn new_sequence(
        meta_collection: &Collection<SequenceMeta>,
    ) -> mongodb::error::Result<SequenceMeta> {
        let sequence_id = ObjectId::new();
        let initial_meta = SequenceMeta {
            object_id: sequence_id,
            meta: true,
            creation_time: BsonDateTime::now(),
            sender_seq_number: 0,
            target_seq_number: 0,
        };
        meta_collection.insert_one(&initial_meta).await?;

        Ok(initial_meta)
    }

    /// Deletes sequences older than the specified age, along with their associated messages.
    ///
    /// This method is useful for cleaning up old session data from MongoDB.
    /// The current active sequence is never deleted, even if it matches the age criteria.
    ///
    /// # Arguments
    ///
    /// * `age` - The minimum age of sequences to delete
    ///
    /// # Returns
    ///
    /// The number of deleted sequences.
    ///
    /// # Errors
    ///
    /// Returns `StoreError::Cleanup` if the cleanup operation fails.
    pub async fn cleanup_older_than(&self, age: Duration) -> Result<u64> {
        cleanup_older_than_inner(
            &self.meta_collection,
            &self.message_collection,
            age,
            Some(self.current_sequence.object_id),
        )
        .await
    }
}

/// Deletes sequences older than the specified age, along with their associated messages.
///
/// This function is useful for cleaning up old session data from MongoDB without
/// needing to instantiate a full [`MongoDbMessageStore`].
/// The latest sequence is never deleted, even if it matches the age criteria.
///
/// # Arguments
///
/// * `db` - The MongoDB database to use
/// * `collection_name` - Optional collection name (defaults to "messages")
/// * `age` - The minimum age of sequences to delete
///
/// # Returns
///
/// The number of deleted sequences.
///
/// # Errors
///
/// Returns `StoreError::Cleanup` if the cleanup operation fails.
pub async fn cleanup_older_than(
    db: &Database,
    collection_name: Option<&str>,
    age: Duration,
) -> Result<u64> {
    let collection_name = collection_name.unwrap_or(DEFAULT_COLLECTION_NAME);
    let meta_collection: Collection<SequenceMeta> = db.collection(collection_name);
    let message_collection: Collection<Message> = db.collection(collection_name);

    // Find latest sequence to exclude
    let options = FindOneOptions::builder().sort(doc! { "_id": -1 }).build();
    let latest = meta_collection
        .find_one(doc! { "meta": true })
        .with_options(options)
        .await
        .map_err(|e| StoreError::Cleanup(e.into()))?;
    let exclude_id = latest.map(|m| m.object_id);

    cleanup_older_than_inner(&meta_collection, &message_collection, age, exclude_id).await
}

async fn cleanup_older_than_inner(
    meta_collection: &Collection<SequenceMeta>,
    message_collection: &Collection<Message>,
    age: Duration,
    exclude_id: Option<ObjectId>,
) -> Result<u64> {
    let cutoff = BsonDateTime::from_millis((Utc::now() - age).timestamp_millis());

    // Find old sequence IDs (excluding the specified sequence if any)
    let mut filter = doc! {
        "meta": true,
        "creation_time": { "$lt": cutoff },
    };
    if let Some(id) = exclude_id {
        filter.insert("_id", doc! { "$ne": id });
    }

    let mut cursor = meta_collection
        .find(filter)
        .await
        .map_err(|e| StoreError::Cleanup(e.into()))?;

    let mut old_sequence_ids = Vec::new();
    while let Some(meta) = cursor
        .try_next()
        .await
        .map_err(|e| StoreError::Cleanup(e.into()))?
    {
        old_sequence_ids.push(meta.object_id);
    }

    if old_sequence_ids.is_empty() {
        return Ok(0);
    }

    // Delete messages first to avoid orphaned meta documents
    message_collection
        .delete_many(doc! { "sequence_id": { "$in": &old_sequence_ids } })
        .await
        .map_err(|e| StoreError::Cleanup(e.into()))?;

    // Delete sequence metas
    let result = meta_collection
        .delete_many(doc! { "_id": { "$in": &old_sequence_ids } })
        .await
        .map_err(|e| StoreError::Cleanup(e.into()))?;

    Ok(result.deleted_count)
}

#[async_trait]
impl MessageStore for MongoDbMessageStore {
    async fn add(&mut self, sequence_number: u64, message: &[u8]) -> Result<()> {
        let message = Message {
            sequence_id: self.current_sequence.object_id,
            msg_seq_number: sequence_number,
            data: Binary {
                subtype: BinarySubtype::Generic,
                bytes: message.to_vec(),
            },
        };
        let filter = doc! { "sequence_id": self.current_sequence.object_id, "msg_seq_number": sequence_number as i64 };
        let options = ReplaceOptions::builder().upsert(true).build();
        self.message_collection
            .replace_one(filter, message)
            .with_options(options)
            .await
            .map_err(|e| StoreError::PersistMessage {
                sequence_number,
                source: e.into(),
            })?;

        Ok(())
    }

    async fn get_slice(&self, begin: usize, end: usize) -> Result<Vec<Vec<u8>>> {
        let filter = doc! {
            "sequence_id": self.current_sequence.object_id,
            "msg_seq_number": doc! {
                "$gte": begin as i64,
                "$lte": end as i64,
            }
        };
        let mut cursor = self.message_collection.find(filter).await.map_err(|e| {
            StoreError::RetrieveMessages {
                begin,
                end,
                source: e.into(),
            }
        })?;

        let mut messages = Vec::new();
        while let Some(message) =
            cursor
                .try_next()
                .await
                .map_err(|e| StoreError::RetrieveMessages {
                    begin,
                    end,
                    source: e.into(),
                })?
        {
            messages.push(message.data.bytes);
        }

        Ok(messages)
    }

    fn next_sender_seq_number(&self) -> u64 {
        self.current_sequence.sender_seq_number + 1
    }

    fn next_target_seq_number(&self) -> u64 {
        self.current_sequence.target_seq_number + 1
    }

    async fn increment_sender_seq_number(&mut self) -> Result<()> {
        self.meta_collection
            .update_one(
                doc! { "_id": self.current_sequence.object_id },
                doc! { "$inc": { "sender_seq_number": 1 } },
            )
            .await
            .map_err(|e| StoreError::UpdateSequenceNumber(e.into()))?;
        self.current_sequence.sender_seq_number += 1;

        Ok(())
    }

    async fn increment_target_seq_number(&mut self) -> Result<()> {
        self.meta_collection
            .update_one(
                doc! { "_id": self.current_sequence.object_id },
                doc! { "$inc": { "target_seq_number": 1 } },
            )
            .await
            .map_err(|e| StoreError::UpdateSequenceNumber(e.into()))?;
        self.current_sequence.target_seq_number += 1;

        Ok(())
    }

    async fn set_target_seq_number(&mut self, seq_number: u64) -> Result<()> {
        self.meta_collection
            .update_one(
                doc! { "_id": self.current_sequence.object_id },
                doc! { "$set": { "target_seq_number": seq_number as i64 } },
            )
            .await
            .map_err(|e| StoreError::UpdateSequenceNumber(e.into()))?;
        self.current_sequence.target_seq_number = seq_number;

        Ok(())
    }

    async fn reset(&mut self) -> Result<()> {
        self.current_sequence = Self::new_sequence(&self.meta_collection)
            .await
            .map_err(|e| StoreError::Reset(e.into()))?;
        Ok(())
    }

    fn creation_time(&self) -> DateTime<Utc> {
        #[allow(clippy::expect_used)]
        Utc.timestamp_millis_opt(self.current_sequence.creation_time.timestamp_millis())
            .single()
            .expect("BsonDateTime is guaranteed to store valid timestamp")
    }
}
