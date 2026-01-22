use crate::store::StoreError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Schedule configuration is invalid: {0}")]
    InvalidSchedule(String),

    #[error("store operation failed")]
    Store(#[from] StoreError),
}

pub type Result<T> = std::result::Result<T, SessionError>;
