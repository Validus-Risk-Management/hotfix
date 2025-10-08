use crate::store::MessageStore;
use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use redb::{Database, ReadOnlyTable, ReadableDatabase, TableDefinition, TableError};
use std::path::Path;

const MESSAGES_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("messages");
const META_TABLE: TableDefinition<&str, u64> = TableDefinition::new("seq_numbers");
const SENDER_KEY: &str = "sender";
const TARGET_KEY: &str = "target";
const CREATION_TIME_KEY: &str = "creation_time";

struct MetaData {
    creation_time: DateTime<Utc>,
    sender_seq_number: u64,
    target_seq_number: u64,
}

pub struct RedbMessageStore {
    db: Database,
    meta: MetaData,
}

impl RedbMessageStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = Database::create(path)?;

        let meta = if let Some(stored_metadata) = Self::load_metadata(&db)? {
            stored_metadata
        } else {
            Self::persist_default_metadata(&db)?;
            Self::load_metadata(&db)?
                .ok_or_else(|| anyhow::anyhow!("failed to read metadata after initialization"))?
        };

        Ok(Self { db, meta })
    }

    fn persist_default_metadata(db: &Database) -> Result<()> {
        let creation_timestamp = Utc::now().timestamp_micros() as u64;
        let sender_seq_number = 0;
        let target_seq_number = 0;

        // if we have just set the creation time, we need to write it to redb
        let write_txn = db.begin_write()?;
        {
            let mut meta_table = write_txn.open_table(META_TABLE)?;
            meta_table.insert(CREATION_TIME_KEY, creation_timestamp)?;
            meta_table.insert(SENDER_KEY, sender_seq_number)?;
            meta_table.insert(TARGET_KEY, target_seq_number)?;
            let mut messages_table = write_txn.open_table(MESSAGES_TABLE)?;
            messages_table.extract_if(|_, _| true)?.for_each(drop);
        }
        write_txn.commit()?;
        Ok(())
    }

    fn load_metadata(db: &Database) -> Result<Option<MetaData>> {
        let read_txn = db.begin_read()?;
        let metadata = match read_txn.open_table(META_TABLE) {
            Ok(table) => {
                let creation_time = Self::parse_timestamp(Self::read_required_meta_field(
                    &table,
                    CREATION_TIME_KEY,
                )?)?;
                let sender_seq_number = Self::read_required_meta_field(&table, SENDER_KEY)?;
                let target_seq_number = Self::read_required_meta_field(&table, TARGET_KEY)?;

                Some(MetaData {
                    creation_time,
                    sender_seq_number,
                    target_seq_number,
                })
            }
            Err(TableError::TableDoesNotExist(_)) => None,
            Err(err) => {
                return Err(err.into());
            }
        };

        Ok(metadata)
    }

    fn read_required_meta_field(table: &ReadOnlyTable<&str, u64>, key: &str) -> Result<u64> {
        table
            .get(key)?
            .map(|v| v.value())
            .ok_or_else(|| anyhow::anyhow!("missing required metadata field: {key}"))
    }

    fn parse_timestamp(timestamp: u64) -> Result<DateTime<Utc>> {
        DateTime::from_timestamp_micros(timestamp as i64)
            .ok_or_else(|| anyhow::anyhow!("invalid timestamp: {timestamp}"))
    }

    async fn update_sequence_number(&mut self, key: &str, value: u64) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(META_TABLE)?;
            table.insert(key, value)?;
        }
        write_txn.commit()?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl MessageStore for RedbMessageStore {
    async fn add(&mut self, sequence_number: u64, message: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(MESSAGES_TABLE)?;
            table.insert(sequence_number, message)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    async fn get_slice(&self, begin: usize, end: usize) -> Result<Vec<Vec<u8>>> {
        if begin > end {
            return Ok(vec![]);
        }

        let read_txn = self.db.begin_read()?;
        match read_txn.open_table(MESSAGES_TABLE) {
            Ok(table) => {
                let messages: std::result::Result<Vec<Vec<u8>>, redb::StorageError> = table
                    .range(begin as u64..=end as u64)?
                    .map(|m| m.map(|v| v.1.value().to_vec()))
                    .collect();
                Ok(messages?)
            }
            Err(TableError::TableDoesNotExist(_)) => Ok(vec![]),
            Err(err) => Err(err.into()),
        }
    }

    fn next_sender_seq_number(&self) -> u64 {
        self.meta.sender_seq_number + 1
    }

    fn next_target_seq_number(&self) -> u64 {
        self.meta.target_seq_number + 1
    }

    async fn increment_sender_seq_number(&mut self) -> Result<()> {
        let sender_seq_number = self.meta.sender_seq_number + 1;
        self.update_sequence_number(SENDER_KEY, sender_seq_number)
            .await?;
        self.meta.sender_seq_number = sender_seq_number;
        Ok(())
    }

    async fn increment_target_seq_number(&mut self) -> Result<()> {
        self.set_target_seq_number(self.meta.target_seq_number + 1)
            .await
    }

    async fn set_target_seq_number(&mut self, seq_number: u64) -> Result<()> {
        self.update_sequence_number(TARGET_KEY, seq_number).await?;
        self.meta.target_seq_number = seq_number;
        Ok(())
    }

    async fn reset(&mut self) -> Result<()> {
        Self::persist_default_metadata(&self.db)?;
        if let Some(meta) = Self::load_metadata(&self.db)? {
            self.meta = meta;
            Ok(())
        } else {
            bail!("meta unexpectedly not found")
        }
    }

    fn creation_time(&self) -> DateTime<Utc> {
        self.meta.creation_time
    }
}
