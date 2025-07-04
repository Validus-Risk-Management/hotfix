use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::spec::BinarySubtype;
use mongodb::bson::Binary;
use mongodb::options::{FindOneOptions, IndexOptions, ReplaceOptions};
use mongodb::{Collection, Database, IndexModel};
use serde::{Deserialize, Serialize};

pub use mongodb::Client;

use crate::store::MessageStore;

#[derive(Debug, Deserialize, Serialize)]
struct SequenceMeta {
    #[serde(rename = "_id")]
    object_id: ObjectId,
    meta: bool,
    creation_time: DateTime<Utc>,
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
    pub async fn new(db: Database, collection_name: Option<&str>) -> Result<Self> {
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

    async fn ensure_indexes(meta_collection: &Collection<SequenceMeta>) -> Result<()> {
        let meta_index = IndexModel::builder()
            .keys(doc! { "meta": 1, "_id": -1 })
            .build();
        let message_index_options = IndexOptions::builder().unique(true).sparse(true).build();
        let message_index = IndexModel::builder()
            .keys(doc! { "sequence_id": 1, "msg_seq_number": 1})
            .options(Some(message_index_options))
            .build();

        meta_collection
            .create_indexes(vec![meta_index, message_index], None)
            .await?;
        Ok(())
    }

    async fn get_or_default_sequence(
        meta_collection: &Collection<SequenceMeta>,
    ) -> Result<SequenceMeta> {
        let options = FindOneOptions::builder().sort(doc! { "_id": -1 }).build();
        let res = meta_collection
            .find_one(doc! { "meta": true }, options)
            .await?;

        let meta = match res {
            None => Self::new_sequence(meta_collection).await?,
            Some(meta) => meta,
        };
        Ok(meta)
    }

    async fn new_sequence(meta_collection: &Collection<SequenceMeta>) -> Result<SequenceMeta> {
        let sequence_id = ObjectId::new();
        let initial_meta = SequenceMeta {
            object_id: sequence_id,
            meta: true,
            creation_time: Utc::now(),
            sender_seq_number: 0,
            target_seq_number: 0,
        };
        meta_collection.insert_one(&initial_meta, None).await?;

        Ok(initial_meta)
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
        let filter = doc! { "sequence_id": self.current_sequence.object_id, "msg_seq_number": sequence_number as u32 };
        let options = ReplaceOptions::builder().upsert(true).build();
        self.message_collection
            .replace_one(filter, message, options)
            .await?;

        Ok(())
    }

    async fn get_slice(&self, begin: usize, end: usize) -> Result<Vec<Vec<u8>>> {
        let filter = doc! {
            "sequence_id": self.current_sequence.object_id,
            "msg_seq_number": doc! {
                "$gte": begin as u32,
                "$lte": end as u32,
            }
        };
        let mut cursor = self.message_collection.find(filter, None).await?;

        let mut messages = Vec::new();
        while let Some(message) = cursor.try_next().await? {
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
        self.current_sequence.sender_seq_number += 1;
        self.meta_collection
            .update_one(
                doc! { "_id": self.current_sequence.object_id },
                doc! { "$inc": { "sender_seq_number": 1 } },
                None,
            )
            .await?;

        Ok(())
    }

    async fn increment_target_seq_number(&mut self) -> Result<()> {
        self.current_sequence.target_seq_number += 1;
        self.meta_collection
            .update_one(
                doc! { "_id": self.current_sequence.object_id },
                doc! { "$inc": { "target_seq_number": 1 } },
                None,
            )
            .await?;

        Ok(())
    }

    async fn set_target_seq_number(&mut self, seq_number: u64) -> Result<()> {
        self.current_sequence.target_seq_number = seq_number;
        self.meta_collection
            .update_one(
                doc! { "_id": self.current_sequence.object_id },
                doc! { "$set": { "target_seq_number": seq_number as u32 } },
                None,
            )
            .await?;

        Ok(())
    }

    async fn reset(&mut self) -> Result<()> {
        self.current_sequence = Self::new_sequence(&self.meta_collection).await?;
        Ok(())
    }

    fn creation_time(&self) -> DateTime<Utc> {
        self.current_sequence.creation_time
    }
}
