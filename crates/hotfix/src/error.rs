use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Schedule configuration is invalid: {0}")]
    InvalidSchedule(String),
}
