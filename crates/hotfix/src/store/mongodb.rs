use async_trait::async_trait;
use mongodb::Client;

use crate::store::MessageStore;

#[allow(dead_code)]
struct MongoDbMessageStore {
    client: Client,
    database_name: String,
    collection_name: String,
}

impl MongoDbMessageStore {
    #[allow(dead_code)]
    pub async fn new(
        client: Client,
        database_name: Option<&str>,
        collection_name: Option<&str>,
    ) -> Self {
        let database_name = database_name.unwrap_or("hotfix");
        let collection_name = collection_name.unwrap_or("messages");

        Self {
            client,
            database_name: database_name.to_string(),
            collection_name: collection_name.to_string(),
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
