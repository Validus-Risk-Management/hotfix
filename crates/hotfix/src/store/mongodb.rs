use async_trait::async_trait;
use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::Binary;
use mongodb::options::FindOneOptions;
use mongodb::{Collection, Database};
use serde::{Deserialize, Serialize};

use crate::store::MessageStore;

#[derive(Debug, Deserialize, Serialize)]
struct SequenceMeta {
    #[serde(rename = "_id")]
    object_id: ObjectId,
}

#[derive(Debug, Deserialize, Serialize)]
struct Message {
    sequence_id: u64,
    data: Binary,
}

#[allow(dead_code)]
struct MongoDbMessageStore {
    meta_collection: Collection<SequenceMeta>,
    message_collection: Collection<Message>,
    sequence_id: ObjectId,
}

impl MongoDbMessageStore {
    #[allow(dead_code)]
    pub async fn new(db: Database, collection_name: Option<&str>) -> Self {
        let collection_name = collection_name.unwrap_or("messages");
        let meta_collection = db.collection(collection_name);
        let message_collection = db.collection(collection_name);

        let sequence_id = Self::get_or_default_sequence(&meta_collection).await;

        Self {
            meta_collection,
            message_collection,
            sequence_id,
        }
    }

    async fn get_or_default_sequence(meta_collection: &Collection<SequenceMeta>) -> ObjectId {
        let options = FindOneOptions::builder().sort(doc! { "_id": -1 }).build();
        let meta = meta_collection.find_one(doc! {}, options).await.unwrap();

        match meta {
            None => {
                let sequence_id = ObjectId::new();
                let initial_meta = SequenceMeta {
                    object_id: sequence_id,
                };
                meta_collection
                    .insert_one(initial_meta, None)
                    .await
                    .unwrap();

                sequence_id
            }
            Some(meta) => meta.object_id,
        }
    }
}

#[async_trait]
impl MessageStore for MongoDbMessageStore {
    async fn add(&mut self, _sequence_number: u64, _message: &[u8]) {
        todo!()
    }

    async fn get_slice(&self, _begin: usize, _end: usize) -> Vec<Vec<u8>> {
        todo!()
    }

    async fn next_sender_seq_number(&self) -> u64 {
        todo!()
    }

    async fn next_target_seq_number(&self) -> u64 {
        todo!()
    }

    async fn increment_sender_seq_number(&mut self) {
        todo!()
    }

    async fn increment_target_seq_number(&mut self) {
        todo!()
    }

    async fn reset(&mut self) {
        todo!()
    }
}
