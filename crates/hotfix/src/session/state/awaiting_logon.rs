use crate::Application;
use crate::message::resend_request::ResendRequest;
use crate::session::ctx::{SessionCtx, TransitionResult, VerificationResult};
use crate::session::error::{InternalSendResultExt, SessionOperationError};
use crate::session::inbound::{self, VerificationOutcome};
use crate::session::outbound;
use crate::session::state::{AwaitingResendState, SessionState};
use crate::transport::writer::WriterRef;
use hotfix_message::message::Message;
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
    pub(crate) async fn handle_verification_issue<A: Application, S: MessageStore>(
        &self,
        ctx: &mut SessionCtx<A, S>,
        message: &Message,
        check_too_high: bool,
        check_too_low: bool,
    ) -> Result<VerificationResult, SessionOperationError> {
        match inbound::verify_and_handle_errors(
            ctx,
            &self.writer,
            message,
            check_too_high,
            check_too_low,
        )
        .await
        {
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
