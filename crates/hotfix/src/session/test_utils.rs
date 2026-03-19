use crate::config::SessionConfig;
use crate::session::ctx::SessionCtx;
use crate::store::{MessageStore, Result as StoreResult};
use crate::transport::writer::{WriterMessage, WriterRef};
use chrono::{DateTime, Utc};
use hotfix_message::MessageBuilder;
use hotfix_message::dict::Dictionary;
use hotfix_message::message::Config as MessageConfig;
use tokio::sync::mpsc;

#[derive(Clone)]
pub(crate) struct FakeMessageStore {
    pub(crate) messages: Vec<Vec<u8>>,
    pub(crate) next_sender_seq: u64,
    pub(crate) next_target_seq: u64,
}

impl FakeMessageStore {
    pub(crate) fn new() -> Self {
        Self {
            messages: vec![],
            next_sender_seq: 1,
            next_target_seq: 1,
        }
    }
}

#[async_trait::async_trait]
impl MessageStore for FakeMessageStore {
    async fn add(&mut self, _: u64, msg: &[u8]) -> StoreResult<()> {
        self.messages.push(msg.to_vec());
        Ok(())
    }
    async fn get_slice(&self, _: usize, _: usize) -> StoreResult<Vec<Vec<u8>>> {
        Ok(self.messages.clone())
    }
    fn next_sender_seq_number(&self) -> u64 {
        self.next_sender_seq
    }
    fn next_target_seq_number(&self) -> u64 {
        self.next_target_seq
    }
    async fn increment_sender_seq_number(&mut self) -> StoreResult<()> {
        self.next_sender_seq += 1;
        Ok(())
    }
    async fn increment_target_seq_number(&mut self) -> StoreResult<()> {
        self.next_target_seq += 1;
        Ok(())
    }
    async fn set_target_seq_number(&mut self, seq: u64) -> StoreResult<()> {
        self.next_target_seq = seq;
        Ok(())
    }
    async fn reset(&mut self) -> StoreResult<()> {
        self.messages.clear();
        self.next_sender_seq = 1;
        self.next_target_seq = 1;
        Ok(())
    }
    fn creation_time(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub(crate) fn create_writer() -> (WriterRef, mpsc::Receiver<WriterMessage>) {
    let (sender, receiver) = mpsc::channel(16);
    (WriterRef::new(sender), receiver)
}

pub(crate) fn create_test_ctx(store: FakeMessageStore) -> SessionCtx<(), FakeMessageStore> {
    let message_config = MessageConfig::default();
    let dictionary = Dictionary::fix44();
    let message_builder = MessageBuilder::new(dictionary, message_config).unwrap();
    SessionCtx {
        config: SessionConfig {
            begin_string: "FIX.4.4".to_string(),
            sender_comp_id: "SENDER".to_string(),
            target_comp_id: "TARGET".to_string(),
            data_dictionary_path: None,
            connection_host: "localhost".to_string(),
            connection_port: 9876,
            tls_config: None,
            heartbeat_interval: 30,
            logon_timeout: 10,
            logout_timeout: 2,
            reconnect_interval: 30,
            reset_on_logon: false,
            schedule: None,
        },
        store,
        application: (),
        message_builder,
        message_config,
    }
}

/// Extract the FIX message type (tag 35) from a raw FIX message bytes.
pub(crate) fn extract_msg_type(raw: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(raw).ok()?;
    for field in s.split('\x01') {
        if let Some(value) = field.strip_prefix("35=") {
            return Some(value.to_string());
        }
    }
    None
}

/// Extract a string field value by tag number from raw FIX message bytes.
pub(crate) fn extract_field(raw: &[u8], tag: u32) -> Option<String> {
    let s = std::str::from_utf8(raw).ok()?;
    let prefix = format!("{tag}=");
    for field in s.split('\x01') {
        if let Some(value) = field.strip_prefix(&prefix) {
            return Some(value.to_string());
        }
    }
    None
}
