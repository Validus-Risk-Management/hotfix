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
use serde_dynamo::{from_item, from_items, to_item};
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

#[derive(Debug, Deserialize, Serialize)]
struct StoredMessage {
    #[serde(rename = "pk")]
    sequence: String,
    #[serde(rename = "sk")]
    sequence_number: u64,
    message: Vec<u8>,
}

pub struct DynamoMessageStore {
    client: Client,
    table_name: String,
    current_sequence: SequenceMeta,
}

impl DynamoMessageStore {
    pub async fn new(client: Client, table_name: String) -> Result<Self> {
        let sequence_meta = Self::ensure_table(&client, &table_name).await?;

        let store = Self {
            client,
            table_name,
            current_sequence: sequence_meta,
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

    async fn persist_sequence(&self) -> Result<()> {
        let item = to_item(&self.current_sequence)?;
        self.client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .send()
            .await?;

        Ok(())
    }
}

#[async_trait]
impl MessageStore for DynamoMessageStore {
    async fn add(&mut self, sequence_number: u64, message: &[u8]) -> Result<()> {
        let stored_message = StoredMessage {
            sequence: self.current_sequence.sequence.clone(),
            sequence_number,
            message: message.to_vec(),
        };
        let item = to_item(stored_message)?;
        self.client
            .put_item()
            .table_name(&self.table_name)
            .set_item(Some(item))
            .send()
            .await?;

        Ok(())
    }

    async fn get_slice(&self, begin: usize, end: usize) -> Result<Vec<Vec<u8>>> {
        let key_condition_expression = "pk = :sequence AND sk BETWEEN :begin AND :end";
        let output = self
            .client
            .query()
            .table_name(&self.table_name)
            .key_condition_expression(key_condition_expression)
            .expression_attribute_values(
                ":sequence",
                AttributeValue::S(self.current_sequence.sequence.clone()),
            )
            .expression_attribute_values(":begin", AttributeValue::N(begin.to_string()))
            .expression_attribute_values(":end", AttributeValue::N(end.to_string()))
            .send()
            .await?;

        let messages: Vec<StoredMessage> = if let Some(items) = output.items {
            from_items(items)?
        } else {
            vec![]
        };

        Ok(messages.into_iter().map(|m| m.message).collect())
    }

    fn next_sender_seq_number(&self) -> u64 {
        self.current_sequence.sender_seq_number
    }

    fn next_target_seq_number(&self) -> u64 {
        self.current_sequence.target_seq_number
    }

    async fn increment_sender_seq_number(&mut self) -> Result<()> {
        // TODO: these increments could use an UpdateExpression
        self.current_sequence.sender_seq_number += 1;
        self.persist_sequence().await
    }

    async fn increment_target_seq_number(&mut self) -> Result<()> {
        self.current_sequence.target_seq_number += 1;
        self.persist_sequence().await
    }

    async fn set_target_seq_number(&mut self, seq_number: u64) -> Result<()> {
        self.current_sequence.target_seq_number = seq_number;
        self.persist_sequence().await
    }

    async fn reset(&mut self) -> Result<()> {
        self.current_sequence = Self::new_sequence(&self.client, &self.table_name).await?;

        Ok(())
    }
}
