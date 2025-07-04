use crate::store::MessageStore;
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use redb::{Database, ReadableTable, TableDefinition, TableError};
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

        let meta = if let Some(stored_metadata) = Self::read_meta_data(&db)? {
            stored_metadata
        } else {
            Self::persist_default_meta_data(&db)?;
            Self::read_meta_data(&db)?.unwrap()
        };

        Ok(Self { db, meta })
    }

    fn persist_default_meta_data(db: &Database) -> Result<()> {
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
        }
        write_txn.commit()?;
        Ok(())
    }

    fn read_meta_data(db: &Database) -> Result<Option<MetaData>> {
        let read_txn = db.begin_read()?;
        let metadata = match read_txn.open_table(META_TABLE) {
            Ok(table) => {
                let creation_time = if let Some(v) = table.get(CREATION_TIME_KEY)? {
                    if let Some(ts) = DateTime::from_timestamp_micros(v.value() as i64) {
                        ts
                    } else {
                        bail!("invalid creation timestamp found")
                    }
                } else {
                    bail!("no creation timestamp found")
                };
                let sender_seq_number = if let Some(v) = table.get(SENDER_KEY)? {
                    v.value()
                } else {
                    bail!("no sender seq number found")
                };
                let target_seq_number = if let Some(v) = table.get(TARGET_KEY)? {
                    v.value()
                } else {
                    bail!("no target seq number found")
                };

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
        let read_txn = self.db.begin_read()?;
        {
            let res = match read_txn.open_table(MESSAGES_TABLE) {
                Ok(table) => {
                    let messages: std::result::Result<Vec<Vec<u8>>, redb::StorageError> = table
                        .range(begin as u64..=end as u64)?
                        .map(|m| m.map(|v| v.1.value().to_vec()))
                        .collect();
                    Ok(messages?)
                }
                Err(TableError::TableDoesNotExist(_)) => Ok(vec![]),
                Err(err) => Err(err.into()),
            };
            res
        }
    }

    fn next_sender_seq_number(&self) -> u64 {
        self.meta.sender_seq_number + 1
    }

    fn next_target_seq_number(&self) -> u64 {
        self.meta.target_seq_number + 1
    }

    async fn increment_sender_seq_number(&mut self) -> Result<()> {
        self.meta.sender_seq_number += 1;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(META_TABLE)?;
            table.insert(SENDER_KEY, self.meta.sender_seq_number)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    async fn increment_target_seq_number(&mut self) -> Result<()> {
        self.meta.target_seq_number += 1;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(META_TABLE)?;
            table.insert(TARGET_KEY, self.meta.target_seq_number)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    async fn set_target_seq_number(&mut self, seq_number: u64) -> Result<()> {
        self.meta.target_seq_number = seq_number;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(META_TABLE)?;
            table.insert(TARGET_KEY, seq_number)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    async fn reset(&mut self) -> Result<()> {
        Self::persist_default_meta_data(&self.db)?;
        if let Some(meta) = Self::read_meta_data(&self.db)? {
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
