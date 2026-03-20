use crate::Application;
use crate::message::resend_request::ResendRequest;
use crate::session::ctx::{SessionCtx, TransitionResult, VerificationResult};
use crate::session::error::{InternalSendResultExt, SessionOperationError};
use crate::session::inbound::{self, VerificationOutcome};
use crate::session::outbound;
use crate::session::state::SessionState;
use crate::transport::writer::WriterRef;
use hotfix_message::message::Message;
use hotfix_store::MessageStore;
use std::collections::VecDeque;
use tracing::debug;

const MAX_RESEND_ATTEMPTS: usize = 3;

/// Session state we're in while processing messages we requested to be resent.
pub(crate) struct AwaitingResendState {
    /// The reference to the writer loop.
    pub(crate) writer: WriterRef,
    /// The beginning of the gap we're waiting for the target to resend.
    pub(crate) begin_seq_number: u64,
    /// The end of the gap we're waiting for the target to resend.
    pub(crate) end_seq_number: u64,
    /// Inbound messages we receive while processing the resend.
    pub(crate) inbound_queue: VecDeque<Message>,
    /// The number of times we've attempted to ask the counterparty to resend the gap.
    pub(crate) resend_attempts: usize,
}

impl AwaitingResendState {
    pub(crate) fn new(writer: WriterRef, begin_seq_number: u64, end_seq_number: u64) -> Self {
        Self {
            writer,
            begin_seq_number,
            end_seq_number,
            inbound_queue: Default::default(),
            resend_attempts: 1,
        }
    }

    pub(crate) async fn handle_verification_issue<A: Application, S: MessageStore>(
        &mut self,
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
                match self.update(expected, actual) {
                    AwaitingResendTransitionOutcome::Success => {
                        debug!(
                            "we are behind target (ours: {expected}, theirs: {actual}), requesting resend."
                        );
                        let request = ResendRequest::new(expected, actual);
                        outbound::send_message(ctx, &self.writer, request)
                            .await
                            .with_send_context("resend request")?;
                        Ok(VerificationResult::Issue(TransitionResult::Stay))
                    }
                    AwaitingResendTransitionOutcome::BeginSeqNumberTooLow => {
                        self.writer.disconnect().await;
                        Ok(VerificationResult::Issue(TransitionResult::TransitionTo(
                            SessionState::new_disconnected(
                                false,
                                "awaiting resend begin seq number unexpectedly lower than the previous resend request's",
                            ),
                        )))
                    }
                    AwaitingResendTransitionOutcome::AttemptsExceeded => {
                        self.writer.disconnect().await;
                        Ok(VerificationResult::Issue(TransitionResult::TransitionTo(
                            SessionState::new_disconnected(
                                false,
                                "resend request attempts exceeded, manual intervention required",
                            ),
                        )))
                    }
                }
            }
        }
    }

    pub(crate) fn update(
        &mut self,
        begin_seq_number: u64,
        end_seq_number: u64,
    ) -> AwaitingResendTransitionOutcome {
        let resend_attempts = if self.begin_seq_number == begin_seq_number {
            if self.resend_attempts + 1 > MAX_RESEND_ATTEMPTS {
                return AwaitingResendTransitionOutcome::AttemptsExceeded;
            }
            self.resend_attempts + 1
        } else if begin_seq_number < self.begin_seq_number {
            return AwaitingResendTransitionOutcome::BeginSeqNumberTooLow;
        } else {
            1
        };

        self.resend_attempts = resend_attempts;
        self.begin_seq_number = begin_seq_number;
        self.end_seq_number = end_seq_number;

        AwaitingResendTransitionOutcome::Success
    }
}

pub(crate) enum AwaitingResendTransitionOutcome {
    Success,
    BeginSeqNumberTooLow,
    AttemptsExceeded,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn test_update_begin_seq_number_too_low() {
        let writer = create_writer_ref();
        let mut state = AwaitingResendState::new(writer, 1, 5);
        let result = state.update(0, 5);
        assert!(matches!(
            result,
            AwaitingResendTransitionOutcome::BeginSeqNumberTooLow
        ));
    }

    #[test]
    fn test_update_attempts_exceeded() {
        let writer = create_writer_ref();
        let mut state = AwaitingResendState::new(writer, 1, 5);

        // we can update twice more without hitting the limit
        let result = state.update(1, 5);
        assert!(matches!(result, AwaitingResendTransitionOutcome::Success));
        let result = state.update(1, 5);
        assert!(matches!(result, AwaitingResendTransitionOutcome::Success));

        // the fourth time with the same begin seq number, we get an error
        let result = state.update(1, 5);
        assert!(matches!(
            result,
            AwaitingResendTransitionOutcome::AttemptsExceeded
        ));
    }

    fn create_writer_ref() -> WriterRef {
        let (sender, _) = mpsc::channel(10);
        WriterRef::new(sender)
    }
}
