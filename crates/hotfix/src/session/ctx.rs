use hotfix_message::MessageBuilder;
use hotfix_message::message::Config as MessageConfig;
use hotfix_store::MessageStore;

use crate::config::SessionConfig;
use crate::message::OutboundMessage;
use crate::message::generate_message;
use crate::message::parser::RawFixMessage;
use crate::session::error::InternalSendError;
use crate::session::state::SessionState;
use crate::store::StoreError;

pub(crate) enum TransitionResult {
    Stay,
    TransitionTo(SessionState),
}

/// The result of verifying an inbound message via a state variant's
/// `handle_verification_issue` method.
pub(crate) enum VerificationResult {
    /// Verification passed — the caller should proceed with handler logic.
    Passed,
    /// A verification issue was detected and handled. The caller should apply
    /// the transition and skip further processing of this message.
    Issue(TransitionResult),
}

pub(crate) struct SessionCtx<A, S> {
    pub config: SessionConfig,
    pub store: S,
    pub application: A,
    pub message_builder: MessageBuilder,
    pub message_config: MessageConfig,
}

pub(crate) struct PreparedMessage {
    pub seq_num: u64,
    pub raw: RawFixMessage,
}

impl<A, S: MessageStore> SessionCtx<A, S> {
    pub async fn prepare_message(
        &mut self,
        message: impl OutboundMessage,
    ) -> Result<PreparedMessage, InternalSendError> {
        let seq_num = self.store.next_sender_seq_number();
        let msg = generate_message(
            &self.config.begin_string,
            &self.config.sender_comp_id,
            &self.config.target_comp_id,
            seq_num,
            message,
        )
        .map_err(|e| {
            InternalSendError::Persist(StoreError::PersistMessage {
                sequence_number: seq_num,
                source: e.into(),
            })
        })?;

        self.store
            .increment_sender_seq_number()
            .await
            .map_err(InternalSendError::SequenceNumber)?;

        self.store
            .add(seq_num, &msg)
            .await
            .map_err(InternalSendError::Persist)?;

        Ok(PreparedMessage {
            seq_num,
            raw: RawFixMessage::new(msg),
        })
    }
}
