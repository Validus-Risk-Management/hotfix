use hotfix_message::Part;
use hotfix_store::MessageStore;
use tracing::{debug, enabled, error, info};

use crate::message::generate_message;
use crate::message::parser::RawFixMessage;
use crate::message::sequence_reset::SequenceReset;
use crate::message::{is_admin, prepare_message_for_resend};
use crate::session::ctx::SessionCtx;
use crate::session::error::SessionOperationError;
use crate::session::get_msg_seq_num;
use crate::transport::writer::WriterRef;
use hotfix_message::session_fields::MSG_TYPE;

pub async fn send_sequence_reset<A, S: MessageStore>(
    ctx: &mut SessionCtx<A, S>,
    writer: &WriterRef,
    begin: u64,
    end: u64,
) -> Result<(), SessionOperationError> {
    let sequence_reset = SequenceReset {
        gap_fill: true,
        new_seq_no: end,
    };
    let raw_message = generate_message(
        &ctx.config.begin_string,
        &ctx.config.sender_comp_id,
        &ctx.config.target_comp_id,
        begin,
        sequence_reset,
    )?;

    writer
        .send_raw_message(RawFixMessage::new(raw_message))
        .await;
    debug!(begin, end, "sent reset sequence");

    Ok(())
}

pub async fn resend_messages<A, S: MessageStore>(
    ctx: &mut SessionCtx<A, S>,
    writer: &WriterRef,
    begin: u64,
    end: u64,
) -> Result<(), SessionOperationError> {
    info!(begin, end, "resending messages as requested");
    let messages = ctx.store.get_slice(begin as usize, end as usize).await?;

    let no = messages.len();
    debug!(number_of_messages = no, "number of messages");

    let mut reset_start: Option<u64> = None;
    let mut sequence_number = 0;

    for msg in messages {
        let mut message = ctx
            .message_builder
            .build(msg.as_slice())
            .into_message()
            .ok_or_else(|| {
                SessionOperationError::StoredMessageParse(format!(
                    "failed to build message for raw message: {msg:?}"
                ))
            })?;
        sequence_number = get_msg_seq_num(&message);
        let message_type: String = message
            .header()
            .get::<&str>(MSG_TYPE)
            .map_err(|_| SessionOperationError::MissingField("MSG_TYPE"))?
            .to_string();

        if is_admin(&message_type) {
            if reset_start.is_none() {
                reset_start = Some(sequence_number);
            }
            continue;
        }

        if let Some(begin) = reset_start {
            let end = sequence_number;
            log_skipped_admin_messages(begin, end);
            send_sequence_reset(ctx, writer, begin, end).await?;
            reset_start = None;
        }

        if let Err(e) = prepare_message_for_resend(&mut message) {
            error!(
                error = e,
                "failed to prepare message for resend, sending original"
            );
        }
        writer
            .send_raw_message(RawFixMessage::new(message.encode(&ctx.message_config)?))
            .await;

        if enabled!(tracing::Level::DEBUG)
            && let Ok(m) = String::from_utf8(msg.clone())
        {
            debug!(sequence_number, message = m, "resent message");
        }
    }

    if let Some(begin) = reset_start {
        // the final reset if needed
        let end = sequence_number;
        log_skipped_admin_messages(begin, end);
        send_sequence_reset(ctx, writer, begin, end).await?;
    }

    Ok(())
}

fn log_skipped_admin_messages(begin: u64, end: u64) {
    info!(
        begin,
        end, "skipped admin message(s) during resend, requesting reset for these"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SessionConfig;
    use crate::session::ctx::SessionCtx;
    use crate::store::Result as StoreResult;
    use chrono::{DateTime, Utc};
    use hotfix_message::MessageBuilder;
    use hotfix_message::dict::Dictionary;
    use hotfix_message::message::Config as MessageConfig;
    use tokio::sync::mpsc;

    #[derive(Clone)]
    struct GarbledMessageStore {
        messages: Vec<Vec<u8>>,
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

    fn create_test_ctx(store: GarbledMessageStore) -> SessionCtx<(), GarbledMessageStore> {
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

    #[tokio::test]
    async fn resend_messages_returns_error_for_garbled_stored_message() {
        let store = GarbledMessageStore {
            messages: vec![b"not a valid FIX message".to_vec()],
        };
        let mut ctx = create_test_ctx(store);
        let (sender, _receiver) = mpsc::channel(10);
        let writer = WriterRef::new(sender);

        let result = resend_messages(&mut ctx, &writer, 1, 1).await;

        assert!(
            matches!(result, Err(SessionOperationError::StoredMessageParse(_))),
            "expected StoredMessageParse error, got: {result:?}"
        );
    }
}
