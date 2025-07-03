use crate::store::MessageStore;
use anyhow::Result;
use chrono::{DateTime, Utc};
use redb::{Database, ReadableTable, TableDefinition, TableError};
use std::path::Path;

const MESSAGES_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("messages");
const SEQ_NUMBER_TABLE: TableDefinition<&str, u64> = TableDefinition::new("seq_numbers");
const SENDER_KEY: &str = "sender";
const TARGET_KEY: &str = "target";

pub struct RedbMessageStore {
    db: Database,
    sender_seq_number: u64,
    target_seq_number: u64,
}

impl RedbMessageStore {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = Database::create(path)?;
        let sender_seq_number;
        let target_seq_number;

        {
            let read_txn = db.begin_read()?;
            match read_txn.open_table(SEQ_NUMBER_TABLE) {
                Ok(table) => {
                    sender_seq_number = table.get(SENDER_KEY)?.map_or(0, |g| g.value());
                    target_seq_number = table.get(TARGET_KEY)?.map_or(0, |g| g.value());
                }
                Err(TableError::TableDoesNotExist(_)) => {
                    // Tables don't exist yet, initialise to 0
                    sender_seq_number = 0;
                    target_seq_number = 0;
                }
                Err(err) => {
                    return Err(err.into());
                }
            };
        }

        Ok(Self {
            db,
            sender_seq_number,
            target_seq_number,
        })
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
        self.sender_seq_number + 1
    }

    fn next_target_seq_number(&self) -> u64 {
        self.target_seq_number + 1
    }

    async fn increment_sender_seq_number(&mut self) -> Result<()> {
        self.sender_seq_number += 1;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SEQ_NUMBER_TABLE)?;
            table.insert(SENDER_KEY, self.sender_seq_number)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    async fn increment_target_seq_number(&mut self) -> Result<()> {
        self.target_seq_number += 1;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SEQ_NUMBER_TABLE)?;
            table.insert(TARGET_KEY, self.target_seq_number)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    async fn set_target_seq_number(&mut self, seq_number: u64) -> Result<()> {
        self.target_seq_number = seq_number;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(SEQ_NUMBER_TABLE)?;
            table.insert(TARGET_KEY, seq_number)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    async fn reset(&mut self) -> Result<()> {
        self.sender_seq_number = 0;
        self.target_seq_number = 0;
        let write_txn = self.db.begin_write()?;
        {
            let mut seq_no_table = write_txn.open_table(SEQ_NUMBER_TABLE)?;
            seq_no_table.insert(SENDER_KEY, self.sender_seq_number)?;
            seq_no_table.insert(TARGET_KEY, self.target_seq_number)?;
            let mut messages_table = write_txn.open_table(MESSAGES_TABLE)?;
            messages_table.drain::<u64>(..)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    async fn creation_time(&self) -> Result<DateTime<Utc>> {
        todo!()
    }
}

#[cfg(test)]
pub(crate) mod test_utils {
    use super::*;
    use crate::store::tests::TestStoreFactory;
    use std::path::PathBuf;
    use std::{env, fs};

    pub(crate) struct RedbTestStoreFactory {
        db_path: PathBuf,
    }

    impl RedbTestStoreFactory {
        pub(crate) fn new() -> Self {
            let mut temp_path = env::temp_dir();
            temp_path.push(format!("redb_test_{}", uuid::Uuid::new_v4()));
            temp_path.set_extension("db");

            Self { db_path: temp_path }
        }
    }

    impl TestStoreFactory for RedbTestStoreFactory {
        fn create_store(&self) -> Box<dyn MessageStore> {
            Box::new(RedbMessageStore::new(&self.db_path).expect("Failed to create store"))
        }
    }

    impl Drop for RedbTestStoreFactory {
        fn drop(&mut self) {
            // Clean up the database file when the test store is dropped
            let _ = fs::remove_file(&self.db_path);
        }
    }
}
