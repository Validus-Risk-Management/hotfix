use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Schedule configuration is invalid: {0}")]
    InvalidSchedule(String),
}

pub type Result<T> = std::result::Result<T, SessionError>;
