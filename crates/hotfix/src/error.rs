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
    #[allow(dead_code)]
    #[error("incorrect comp id {comp_id} ({comp_id_type:?})")]
    IncorrectCompId {
        comp_id: String,
        comp_id_type: CompIdType,
        msg_seq_num: u64,
    },
}

#[derive(Debug)]
pub enum CompIdType {
    Sender,
    Target,
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Schedule configuration is invalid: {0}")]
    InvalidSchedule(String),
}
