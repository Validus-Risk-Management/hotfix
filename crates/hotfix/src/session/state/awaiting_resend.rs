use crate::transport::writer::WriterRef;
use hotfix_message::message::Message;
use std::collections::VecDeque;

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
    InvalidState(String),
    BeginSeqNumberTooLow,
    AttemptsExceeded,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::state::SessionState;
    use tokio::sync::mpsc;
    use tokio::time::Instant;

    #[test]
    fn test_awaiting_resend_transition_begin_seq_number_too_low() {
        let writer = create_writer_ref();
        let mut state = SessionState::AwaitingResend(AwaitingResendState::new(writer, 1, 5));
        let result = state.try_transition_to_awaiting_resend(0, 5);
        assert!(matches!(
            result,
            AwaitingResendTransitionOutcome::BeginSeqNumberTooLow
        ));
    }

    #[test]
    fn test_awaiting_resend_transition_attempts_exceeded() {
        let writer = create_writer_ref();
        let mut state = SessionState::AwaitingResend(AwaitingResendState::new(writer, 1, 5));

        // we can transition twice more without hitting the limit
        let result = state.try_transition_to_awaiting_resend(1, 5);
        assert!(matches!(result, AwaitingResendTransitionOutcome::Success));
        let result = state.try_transition_to_awaiting_resend(1, 5);
        assert!(matches!(result, AwaitingResendTransitionOutcome::Success));

        // the fourth time we'd get into an AwaitingResendState with the same begin seq number, we get an error
        let result = state.try_transition_to_awaiting_resend(1, 5);
        assert!(matches!(
            result,
            AwaitingResendTransitionOutcome::AttemptsExceeded
        ));
    }

    #[test]
    fn test_awaiting_resend_transition_when_awaiting_logout_is_prevented() {
        use crate::session::state::AwaitingLogoutState;

        let mut state = SessionState::AwaitingLogout(AwaitingLogoutState {
            writer: create_writer_ref(),
            logout_timeout: Instant::now(),
            reconnect: false,
        });

        let result = state.try_transition_to_awaiting_resend(1, 5);
        assert!(matches!(
            result,
            AwaitingResendTransitionOutcome::InvalidState(_)
        ));
    }

    fn create_writer_ref() -> WriterRef {
        let (sender, _) = mpsc::channel(10);
        WriterRef::new(sender)
    }
}
