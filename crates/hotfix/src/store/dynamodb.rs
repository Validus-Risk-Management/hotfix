use anyhow::{bail, Result};
use async_trait::async_trait;
use aws_sdk_dynamodb::error::ProvideErrorMetadata;
use aws_sdk_dynamodb::operation::create_table::CreateTableError;
use aws_sdk_dynamodb::types::{
    AttributeDefinition, AttributeValue, BillingMode, KeySchemaElement, KeyType,
    ScalarAttributeType,
};
use aws_sdk_dynamodb::Client;
use serde::{Deserialize, Serialize};
use serde_dynamo::{from_item, to_item};
use tracing::{error, info};

pub use aws_config as config;
pub use aws_sdk_dynamodb as sdk;

use crate::store::MessageStore;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SequenceMeta {
    #[serde(rename = "pk")]
    sequence: String,
    sk: u64,
    sender_seq_number: u64,
    target_seq_number: u64,
}

impl SequenceMeta {
    fn new() -> Self {
        Self {
            sequence: uuid::Uuid::new_v4().hyphenated().to_string(),
            sk: 0,
            sender_seq_number: 1,
            target_seq_number: 1,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct CurrentSequence {
    pk: String,
    sk: u64,
    sequence_name: String,
}

impl CurrentSequence {
    fn new(sequence_name: String) -> Self {
        Self {
            pk: "CURRENT".to_string(),
            sk: 0,
            sequence_name,
        }
    }
}

pub struct DynamoMessageStore {
    client: Client,
    table_name: String,
    sequence_meta: SequenceMeta,
}

impl DynamoMessageStore {
    pub async fn new(client: Client, table_name: String) -> Result<Self> {
        let sequence_meta = Self::ensure_table(&client, &table_name).await?;

        let store = Self {
            client,
            table_name,
            sequence_meta,
        };
        Ok(store)
    }

    async fn ensure_table(client: &Client, table_name: &str) -> Result<SequenceMeta> {
        let is_new = Self::create_table(client, table_name).await?;

        let sequence = if is_new {
            Self::new_sequence(client, table_name).await?
        } else {
            Self::get_current_meta(client, table_name).await?
        };

        Ok(sequence)
    }

    async fn get_current_meta(client: &Client, table_name: &str) -> Result<SequenceMeta> {
        let output = client
            .get_item()
            .table_name(table_name)
            .key("pk", AttributeValue::S("CURRENT".to_string()))
            .key("sk", AttributeValue::N("0".to_string()))
            .send()
            .await?;
        let current_item = output
            .item
            .ok_or(anyhow::anyhow!("no current sequence in database"))?;
        let current: CurrentSequence = from_item(current_item)?;
        let output = client
            .get_item()
            .table_name(table_name)
            .key("pk", AttributeValue::S(current.sequence_name))
            .key("sk", AttributeValue::N("0".to_string()))
            .send()
            .await?;
        let sequence_item = output
            .item
            .ok_or(anyhow::anyhow!("current sequence not found"))?;
        let sequence: SequenceMeta = from_item(sequence_item)?;

        Ok(sequence)
    }

    async fn create_table(client: &Client, table_name: &str) -> Result<bool> {
        let pk = AttributeDefinition::builder()
            .attribute_name("pk")
            .attribute_type(ScalarAttributeType::S)
            .build()?;
        let pk_schema = KeySchemaElement::builder()
            .attribute_name("pk")
            .key_type(KeyType::Hash)
            .build()?;

        let sk = AttributeDefinition::builder()
            .attribute_name("sk")
            .attribute_type(ScalarAttributeType::N)
            .build()?;
        let sk_schema = KeySchemaElement::builder()
            .attribute_name("sk")
            .key_type(KeyType::Range)
            .build()?;

        if let Err(err) = client
            .create_table()
            .billing_mode(BillingMode::PayPerRequest)
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
                    return Ok(false);
                }
                err => {
                    let message = err.message().unwrap_or("no message");
                    error!(message, "failed to create table");
                    bail!("failed to create table");
                }
            }
        }

        info!("created DynamoDB table {table_name}");
        Ok(true)
    }

    async fn new_sequence(client: &Client, table_name: &str) -> Result<SequenceMeta> {
        let sequence_meta = SequenceMeta::new();
        let new_sequence = to_item(sequence_meta.clone())?;
        client
            .put_item()
            .table_name(table_name)
            .set_item(Some(new_sequence))
            .send()
            .await?;
        let current_sequence = to_item(CurrentSequence::new(sequence_meta.sequence.clone()))?;
        client
            .put_item()
            .table_name(table_name)
            .set_item(Some(current_sequence))
            .send()
            .await?;

        Ok(sequence_meta)
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
        self.sequence_meta = Self::new_sequence(&self.client, &self.table_name).await?;

        Ok(())
    }
}
