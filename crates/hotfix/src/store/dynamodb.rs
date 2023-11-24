use anyhow::{bail, Result};
use async_trait::async_trait;
use aws_sdk_dynamodb::error::ProvideErrorMetadata;
use aws_sdk_dynamodb::operation::create_table::CreateTableError;
use aws_sdk_dynamodb::types::{
    AttributeDefinition, KeySchemaElement, KeyType, ScalarAttributeType,
};
use aws_sdk_dynamodb::Client;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::store::MessageStore;

#[derive(Debug, Deserialize, Serialize)]
struct SequenceMeta {
    sequence: String,
    sender_seq_number: u64,
    target_seq_number: u64,
}

pub struct DynamoMessageStore {
    client: Client,
    table_name: String,
}

impl DynamoMessageStore {
    pub async fn new(client: Client, table_name: String) -> Result<Self> {
        Self::ensure_table(&client, &table_name).await?;

        let store = Self { client, table_name };
        Ok(store)
    }

    async fn ensure_table(client: &Client, table_name: &String) -> Result<()> {
        if let Some(table_names) = client.list_tables().send().await?.table_names {
            if table_names.contains(table_name) {
                return Ok(());
            }
        };

        let pk = AttributeDefinition::builder()
            .attribute_name("sequence_name")
            .attribute_type(ScalarAttributeType::S)
            .build()?;
        let pk_schema = KeySchemaElement::builder()
            .attribute_name("sequence_name")
            .key_type(KeyType::Hash)
            .build()?;

        let sk = AttributeDefinition::builder()
            .attribute_name("sequence_number")
            .attribute_type(ScalarAttributeType::N)
            .build()?;
        let sk_schema = KeySchemaElement::builder()
            .attribute_name("sequence_number")
            .key_type(KeyType::Range)
            .build()?;

        if let Err(err) = client
            .create_table()
            .table_name(table_name)
            .key_schema(pk_schema)
            .attribute_definitions(pk)
            .key_schema(sk_schema)
            .attribute_definitions(sk)
            .send()
            .await
        {
            match err.into_service_error() {
                CreateTableError::ResourceInUseException(_) => {
                    info!("DynamoDB table already exists, not creating a new one");
                }
                err => {
                    let message = err.message().unwrap_or("no message");
                    error!(message, "failed to create table");
                    bail!("failed to create table");
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl MessageStore for DynamoMessageStore {
    async fn add(&mut self, _sequence_number: u64, _message: &[u8]) -> Result<()> {
        todo!()
    }

    async fn get_slice(&self, _begin: usize, _end: usize) -> Result<Vec<Vec<u8>>> {
        todo!()
    }

    fn next_sender_seq_number(&self) -> u64 {
        todo!()
    }

    fn next_target_seq_number(&self) -> u64 {
        todo!()
    }

    async fn increment_sender_seq_number(&mut self) -> Result<()> {
        todo!()
    }

    async fn increment_target_seq_number(&mut self) -> Result<()> {
        todo!()
    }

    async fn set_target_seq_number(&mut self, _seq_number: u64) -> Result<()> {
        todo!()
    }

    async fn reset(&mut self) -> Result<()> {
        todo!()
    }
}
