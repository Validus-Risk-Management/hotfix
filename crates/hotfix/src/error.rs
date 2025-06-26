use thiserror::Error;

#[derive(Debug, Error)]
pub enum MessageVerificationError {
    /// The message's sequence number is lower than we expected.
    #[error("sequence number too low (expected {expected:?}, actual {actual:?})")]
    SeqNumberTooLow { expected: u64, actual: u64 },

    /// The message's sequence number is higher than we expected.
    #[error("sequence number too high (expected {expected:?}, actual {actual:?})")]
    SeqNumberTooHigh { expected: u64, actual: u64 },

    /// The begin string is different from our expectations.
    #[error("incorrect begin string {0}")]
    IncorrectBeginString(String),

    /// The comp ID is different from our expectations.
    #[allow(dead_code)]
    #[error("incorrect comp id {0}")]
    IncorrectCompId(String),
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Schedule configuration is invalid: {0}")]
    InvalidSchedule(String),
}
