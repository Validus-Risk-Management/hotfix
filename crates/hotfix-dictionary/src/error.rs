pub(crate) type ParseResult<T> = Result<T, ParseError>;

/// The error type that can arise when decoding a QuickFIX Dictionary.
#[derive(Clone, Debug, thiserror::Error)]
pub enum ParseError {
    #[error("invalid format")]
    InvalidFormat,
    #[error("invalid data: {0}")]
    InvalidData(String),
}
