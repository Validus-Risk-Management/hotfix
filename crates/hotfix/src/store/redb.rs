use redb::TableError::TableDoesNotExist;
use redb::{Database, ReadableTable, TableDefinition};
use std::path::Path;

use crate::store::MessageStore;

const MESSAGES_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("messages");
const SEQ_NUMBER_TABLE: TableDefinition<&str, u64> = TableDefinition::new("seq_numbers");

pub struct RedbMessageStore {
    db: Database,
}

impl RedbMessageStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let db = Database::create(path).expect("be able to create database");

        Self { db }
    }
}

#[async_trait::async_trait]
impl MessageStore for RedbMessageStore {
    async fn add(&mut self, sequence_number: u64, message: &[u8]) {
        let write_txn = self.db.begin_write().unwrap();
        {
            let mut table = write_txn.open_table(MESSAGES_TABLE).unwrap();
            table.insert(sequence_number, message).unwrap();
        }
        write_txn.commit().unwrap();
    }

    async fn get_slice(&self, begin: usize, end: usize) -> Vec<Vec<u8>> {
        let read_txn = self.db.begin_read().unwrap();
        {
            let table = read_txn.open_table(MESSAGES_TABLE).unwrap();
            table
                .range(begin as u64..=end as u64)
                .unwrap()
                .map(|m| m.unwrap().1.value().to_vec())
                .collect()
        }
    }

    async fn next_sender_seq_number(&self) -> u64 {
        let read_txn = self.db.begin_read().unwrap();
        let opened_table = read_txn.open_table(SEQ_NUMBER_TABLE);
        match opened_table {
            Ok(table) => {
                let value = table.get("sender").unwrap();
                match value {
                    None => 1,
                    Some(v) => v.value() + 1,
                }
            }
            Err(TableDoesNotExist(_)) => 1,
            Err(err) => panic!("{}", err.to_string()),
        }
    }

    async fn next_target_seq_number(&self) -> u64 {
        let read_txn = self.db.begin_read().unwrap();
        let opened_table = read_txn.open_table(SEQ_NUMBER_TABLE);
        match opened_table {
            Ok(table) => {
                let value = table.get("target").unwrap();
                match value {
                    None => 1,
                    Some(v) => v.value() + 1,
                }
            }
            Err(TableDoesNotExist(_)) => 1,
            Err(err) => panic!("{}", err.to_string()),
        }
    }

    async fn increment_sender_seq_number(&mut self) {
        let write_txn = self.db.begin_write().unwrap();
        {
            let mut table = write_txn.open_table(SEQ_NUMBER_TABLE).unwrap();
            let current = match table.get("sender").unwrap() {
                None => 0,
                Some(v) => v.value(),
            };
            table.insert("sender", current + 1).unwrap();
        }
        write_txn.commit().unwrap();
    }

    async fn increment_target_seq_number(&mut self) {
        let write_txn = self.db.begin_write().unwrap();
        {
            let mut table = write_txn.open_table(SEQ_NUMBER_TABLE).unwrap();
            let current = match table.get("target").unwrap() {
                None => 0,
                Some(v) => v.value(),
            };
            table.insert("target", current + 1).unwrap();
        }
        write_txn.commit().unwrap();
    }

    async fn reset(&mut self) {
        let write_txn = self.db.begin_write().unwrap();
        {
            let mut seq_no_table = write_txn.open_table(SEQ_NUMBER_TABLE).unwrap();
            seq_no_table.insert("sender", 0).unwrap();
            seq_no_table.insert("target", 0).unwrap();
            let mut messages_table = write_txn.open_table(MESSAGES_TABLE).unwrap();
            messages_table.drain::<u64>(..).unwrap();
        }
        write_txn.commit().unwrap();
    }
}
