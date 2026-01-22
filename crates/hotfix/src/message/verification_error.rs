use hotfix_message::field_types::Timestamp;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MessageVerificationError {
    /// The message's sequence number is lower than we expected.
    #[error(
        "sequence number too low (expected {expected:?}, actual {actual:?}, possible duplicate: {possible_duplicate})"
    )]
    SeqNumberTooLow {
        expected: u64,
        actual: u64,
        possible_duplicate: bool,
    },

    /// The message's sequence number is higher than we expected.
    #[error("sequence number too high (expected {expected:?}, actual {actual:?})")]
    SeqNumberTooHigh { expected: u64, actual: u64 },

    /// The begin string is different from our expectations.
    #[error("incorrect begin string {0}")]
    IncorrectBeginString(String),

    /// The comp ID is different from our expectations.
    #[error("incorrect comp id {comp_id} ({comp_id_type:?})")]
    IncorrectCompId {
        comp_id: String,
        comp_id_type: CompIdType,
        msg_seq_num: u64,
    },
    /// The sending time is not within the latency threshold.
    #[error("sending time accuracy issue")]
    SendingTimeAccuracyIssue { msg_seq_num: u64 },
    /// The sending time field is missing from the message.
    #[error("sending time missing")]
    SendingTimeMissing { msg_seq_num: u64 },
    /// Original sending time is not provided despite PossDupFlag being set.
    #[error("original sending time missing")]
    OriginalSendingTimeMissing { msg_seq_num: u64 },
    /// The original sending time is after the sending time of the message.
    #[error(
        "original sending time {original_sending_time:?} is after sending time {sending_time:?}"
    )]
    OriginalSendingTimeAfterSendingTime {
        msg_seq_num: u64,
        original_sending_time: Timestamp,
        sending_time: Timestamp,
    },
}

#[derive(Debug)]
pub enum CompIdType {
    Sender,
    Target,
}
