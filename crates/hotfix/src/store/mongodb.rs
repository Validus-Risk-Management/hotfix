use async_trait::async_trait;
use futures::TryStreamExt;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::spec::BinarySubtype;
use mongodb::bson::Binary;
use mongodb::options::FindOneOptions;
use mongodb::{Collection, Database};
use serde::{Deserialize, Serialize};

pub use mongodb::Client;

use crate::store::MessageStore;

#[derive(Debug, Deserialize, Serialize)]
struct SequenceMeta {
    #[serde(rename = "_id")]
    object_id: ObjectId,
    meta: bool,
    sender_seq_number: u64,
    target_seq_number: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct Message {
    sequence_id: ObjectId,
    msg_seq_number: u64,
    data: Binary,
}

#[allow(dead_code)]
pub struct MongoDbMessageStore {
    meta_collection: Collection<SequenceMeta>,
    message_collection: Collection<Message>,
    current_sequence: SequenceMeta,
}

impl MongoDbMessageStore {
    #[allow(dead_code)]
    pub async fn new(db: Database, collection_name: Option<&str>) -> Self {
        let collection_name = collection_name.unwrap_or("messages");
        let meta_collection = db.collection(collection_name);
        let message_collection = db.collection(collection_name);

        let current_sequence = Self::get_or_default_sequence(&meta_collection).await;

        Self {
            meta_collection,
            message_collection,
            current_sequence,
        }
    }

    async fn get_or_default_sequence(meta_collection: &Collection<SequenceMeta>) -> SequenceMeta {
        let options = FindOneOptions::builder().sort(doc! { "_id": -1 }).build();
        let meta = meta_collection
            .find_one(doc! { "meta": true }, options)
            .await
            .unwrap();

        match meta {
            None => Self::new_sequence(meta_collection).await,
            Some(meta) => meta,
        }
    }

    async fn new_sequence(meta_collection: &Collection<SequenceMeta>) -> SequenceMeta {
        let sequence_id = ObjectId::new();
        let initial_meta = SequenceMeta {
            object_id: sequence_id,
            meta: true,
            sender_seq_number: 1,
            target_seq_number: 1,
        };
        meta_collection
            .insert_one(&initial_meta, None)
            .await
            .unwrap();

        initial_meta
    }
}

#[async_trait]
impl MessageStore for MongoDbMessageStore {
    async fn add(&mut self, sequence_number: u64, message: &[u8]) {
        let message = Message {
            sequence_id: Default::default(),
            msg_seq_number: sequence_number,
            data: Binary {
                subtype: BinarySubtype::Generic,
                bytes: message.to_vec(),
            },
        };
        self.message_collection
            .insert_one(message, None)
            .await
            .unwrap();
    }

    async fn get_slice(&self, begin: usize, end: usize) -> Vec<Vec<u8>> {
        let filter = doc! {
            "sequence_id": self.current_sequence.object_id,
            "msg_seq_number": doc! {
                "$gt": begin as u32,
                "$lt": end as u32,
            }
        };
        let mut cursor = self.message_collection.find(filter, None).await.unwrap();

        let mut messages = Vec::new();
        while let Some(message) = cursor.try_next().await.unwrap() {
            messages.push(message.data.bytes);
        }

        messages
    }

    async fn next_sender_seq_number(&self) -> u64 {
        self.current_sequence.sender_seq_number
    }

    async fn next_target_seq_number(&self) -> u64 {
        self.current_sequence.target_seq_number
    }

    async fn increment_sender_seq_number(&mut self) {
        self.current_sequence.sender_seq_number += 1;
        self.meta_collection
            .update_one(
                doc! { "_id": self.current_sequence.object_id },
                doc! { "$inc": { "sender_seq_number": 1 } },
                None,
            )
            .await
            .unwrap();
    }

    async fn increment_target_seq_number(&mut self) {
        self.current_sequence.target_seq_number += 1;
        self.meta_collection
            .update_one(
                doc! { "_id": self.current_sequence.object_id },
                doc! { "$inc": { "target_seq_number": 1 } },
                None,
            )
            .await
            .unwrap();
    }

    async fn reset(&mut self) {
        self.current_sequence = Self::new_sequence(&self.meta_collection).await;
    }
}
