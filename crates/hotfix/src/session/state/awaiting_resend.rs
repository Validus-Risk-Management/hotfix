use crate::Application;
use crate::message::resend_request::ResendRequest;
use crate::message::verification::VerificationFlags;
use crate::session::ctx::{PreProcessDecision, SessionCtx, TransitionResult, VerificationResult};
use crate::session::error::{InternalSendResultExt, SessionOperationError};
use crate::session::get_msg_seq_num;
use crate::session::inbound::{self, VerificationOutcome};
use crate::session::outbound;
use crate::session::state::SessionState;
use crate::transport::writer::WriterRef;
use hotfix_message::Part;
use hotfix_message::message::Message;
use hotfix_message::session_fields::MSG_TYPE;
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

    /// Check whether the resend is complete. If the next expected target sequence number
    /// exceeds the end of the gap, return the queued backlog for replay and transition
    /// to Active. Otherwise return `None`.
    pub(crate) fn try_complete(
        &mut self,
        next_target_seq: u64,
        heartbeat_interval: u64,
    ) -> Option<(SessionState, VecDeque<Message>)> {
        if next_target_seq > self.end_seq_number {
            let backlog = std::mem::take(&mut self.inbound_queue);
            let new_state = SessionState::new_active(self.writer.clone(), heartbeat_interval);
            Some((new_state, backlog))
        } else {
            None
        }
    }

    pub(crate) fn pre_process_inbound(&mut self, message: Message) -> PreProcessDecision {
        let dominated_by_resend = message
            .header()
            .get::<&str>(MSG_TYPE)
            .is_ok_and(|t| t != ResendRequest::MSG_TYPE);

        if get_msg_seq_num(&message) > self.end_seq_number && dominated_by_resend {
            self.inbound_queue.push_back(message);
            PreProcessDecision::Queued
        } else {
            PreProcessDecision::Accept(message)
        }
    }

    pub(crate) async fn handle_verification_issue<A: Application, S: MessageStore>(
        &mut self,
        ctx: &mut SessionCtx<A, S>,
        message: &Message,
        flags: VerificationFlags,
    ) -> Result<VerificationResult, SessionOperationError> {
        match inbound::verify_and_handle_errors(ctx, &self.writer, message, flags).await {
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
