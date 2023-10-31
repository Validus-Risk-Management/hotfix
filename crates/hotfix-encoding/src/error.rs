/// The type returned in the event of an error during message decoding.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    /// Mandatory field not found.
    #[error("Field not found.")]
    FieldPresence,
    /// Invalid FIX message syntax, `BodyLength <9>` value mismatch, or similar errors.
    #[error("Invalid FIX message syntax.")]
    Invalid,
    /// Invalid `CheckSum <10>` FIX field value.
    #[error("Invalid `CheckSum <10>` FIX field value.")]
    CheckSum,
    /// I/O error.
    #[error("I/O error: {0}")]
    IO(#[from] std::io::Error),
}
