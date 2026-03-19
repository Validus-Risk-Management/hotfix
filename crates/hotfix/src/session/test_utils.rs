use crate::config::SessionConfig;
use crate::session::ctx::SessionCtx;
use crate::store::{MessageStore, Result as StoreResult};
use chrono::{DateTime, Utc};
use hotfix_message::MessageBuilder;
use hotfix_message::dict::Dictionary;
use hotfix_message::message::Config as MessageConfig;

#[derive(Clone)]
pub(crate) struct GarbledMessageStore {
    pub(crate) messages: Vec<Vec<u8>>,
}

#[async_trait::async_trait]
impl MessageStore for GarbledMessageStore {
    async fn add(&mut self, _: u64, _: &[u8]) -> StoreResult<()> {
        Ok(())
    }
    async fn get_slice(&self, _: usize, _: usize) -> StoreResult<Vec<Vec<u8>>> {
        Ok(self.messages.clone())
    }
    fn next_sender_seq_number(&self) -> u64 {
        1
    }
    fn next_target_seq_number(&self) -> u64 {
        1
    }
    async fn increment_sender_seq_number(&mut self) -> StoreResult<()> {
        Ok(())
    }
    async fn increment_target_seq_number(&mut self) -> StoreResult<()> {
        Ok(())
    }
    async fn set_target_seq_number(&mut self, _: u64) -> StoreResult<()> {
        Ok(())
    }
    async fn reset(&mut self) -> StoreResult<()> {
        Ok(())
    }
    fn creation_time(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub(crate) fn create_test_ctx(store: GarbledMessageStore) -> SessionCtx<(), GarbledMessageStore> {
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
