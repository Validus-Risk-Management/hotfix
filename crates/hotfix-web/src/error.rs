use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum AppError {
    /// General anyhow errors
    Anyhow(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}
