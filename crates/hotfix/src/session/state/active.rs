use crate::Application;
use crate::message::resend_request::ResendRequest;
use crate::message::verification::VerificationFlags;
use crate::session::ctx::{SessionCtx, TransitionResult, VerificationResult};
use crate::session::error::{InternalSendResultExt, SessionOperationError};
use crate::session::inbound::{self, VerificationOutcome};
use crate::session::outbound;
use crate::session::state::{AwaitingResendState, SessionState, TestRequestId};
use crate::transport::writer::WriterRef;
use hotfix_message::message::Message;
use hotfix_store::MessageStore;
use tokio::time::Instant;
use tracing::debug;

pub(crate) struct ActiveState {
    /// The writer's reference to send messages to the counterparty
    pub(crate) writer: WriterRef,
    /// When we should send the next heartbeat message to the counterparty
    pub(crate) heartbeat_deadline: Instant,
    /// When the next message from the counterparty is expected at the latest
    pub(crate) peer_deadline: Instant,
    /// The ID of the test request we sent on peer timer expiry
    pub(crate) sent_test_request_id: Option<TestRequestId>,
}

impl ActiveState {
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

#[inline]
pub(crate) fn calculate_peer_interval(heartbeat_interval: u64) -> u64 {
    (heartbeat_interval as f64 * super::TEST_REQUEST_THRESHOLD).round() as u64
}
