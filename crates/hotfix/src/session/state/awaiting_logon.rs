use crate::Application;
use crate::message::logon::Logon;
use crate::message::resend_request::ResendRequest;
use crate::message::verification::VerificationFlags;
use crate::session::ctx::{PreProcessDecision, SessionCtx, TransitionResult, VerificationResult};
use crate::session::error::{InternalSendResultExt, SessionOperationError};
use crate::session::inbound::{self, VerificationOutcome};
use crate::session::outbound;
use crate::session::state::{AwaitingResendState, SessionState};
use crate::transport::writer::WriterRef;
use hotfix_message::Part;
use hotfix_message::message::Message;
use hotfix_message::session_fields::MSG_TYPE;
use hotfix_store::MessageStore;
use tokio::time::Instant;
use tracing::debug;

pub(crate) struct AwaitingLogonState {
    /// The writer's reference to send messages to the counterparty
    pub(crate) writer: WriterRef,
    /// Indicates whether we have sent Logon - safeguards against accidental double sends
    pub(crate) logon_sent: bool,
    /// When we are expecting the Logon response at the latest
    pub(crate) logon_timeout: Instant,
}

impl AwaitingLogonState {
    pub(crate) fn pre_process_inbound(&self, message: Message) -> PreProcessDecision {
        let is_logon = message
            .header()
            .get::<&str>(MSG_TYPE)
            .is_ok_and(|t| t == Logon::MSG_TYPE);

        if is_logon {
            PreProcessDecision::Accept(message)
        } else {
            PreProcessDecision::Disconnect
        }
    }

    pub(crate) async fn on_peer_logon<A: Application, S: MessageStore>(
        &self,
        ctx: &mut SessionCtx<A, S>,
    ) -> Result<TransitionResult, SessionOperationError> {
        ctx.application.on_logon().await;
        ctx.store.increment_target_seq_number().await?;
        Ok(TransitionResult::TransitionTo(SessionState::new_active(
            self.writer.clone(),
            ctx.config.heartbeat_interval,
        )))
    }

    pub(crate) async fn handle_verification_issue<A: Application, S: MessageStore>(
        &self,
        ctx: &mut SessionCtx<A, S>,
        message: &Message,
        flags: VerificationFlags,
    ) -> Result<VerificationResult, SessionOperationError> {
        match inbound::verify_and_handle_errors(ctx, &self.writer, message, flags).await {
            VerificationOutcome::Ok => Ok(VerificationResult::Passed),
            VerificationOutcome::Handled(result) => Ok(VerificationResult::Issue(result)),
            VerificationOutcome::SequenceGap { expected, actual } => {
                debug!(
                    "we are behind target (ours: {expected}, theirs: {actual}), requesting resend."
                );
                let awaiting_resend =
                    AwaitingResendState::new(self.writer.clone(), expected, actual);
                let request = ResendRequest::new(expected, actual);
                outbound::send_message(ctx, &self.writer, request)
                    .await
                    .with_send_context("resend request")?;
                Ok(VerificationResult::Issue(TransitionResult::TransitionTo(
                    SessionState::AwaitingResend(awaiting_resend),
                )))
            }
        }
    }
}
