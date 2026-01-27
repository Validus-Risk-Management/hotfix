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

pub use mongodb::Client;

use crate::store::{MessageStore, Result, StoreError};

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

pub struct MongoDbMessageStore {
    meta_collection: Collection<SequenceMeta>,
    message_collection: Collection<Message>,
    current_sequence: SequenceMeta,
}

impl MongoDbMessageStore {
    pub async fn new(db: Database, collection_name: Option<&str>) -> anyhow::Result<Self> {
        let collection_name = collection_name.unwrap_or("messages");
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

    async fn ensure_indexes(meta_collection: &Collection<SequenceMeta>) -> anyhow::Result<()> {
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
    ) -> anyhow::Result<SequenceMeta> {
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
    ) -> anyhow::Result<SequenceMeta> {
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
    /// Returns the number of deleted sequences.
    pub async fn cleanup_older_than(&self, age: Duration) -> Result<u64> {
        let cutoff = BsonDateTime::from_millis((Utc::now() - age).timestamp_millis());

        // Find old sequence IDs (excluding current sequence)
        let filter = doc! {
            "meta": true,
            "creation_time": { "$lt": cutoff },
            "_id": { "$ne": self.current_sequence.object_id }
        };
        let mut cursor = self
            .meta_collection
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
        self.message_collection
            .delete_many(doc! { "sequence_id": { "$in": &old_sequence_ids } })
            .await
            .map_err(|e| StoreError::Cleanup(e.into()))?;

        // Delete sequence metas
        let result = self
            .meta_collection
            .delete_many(doc! { "_id": { "$in": &old_sequence_ids } })
            .await
            .map_err(|e| StoreError::Cleanup(e.into()))?;

        Ok(result.deleted_count)
    }
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
