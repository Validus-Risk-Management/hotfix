use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum DashboardError {
    /// General anyhow errors
    Anyhow(#[from] anyhow::Error),
    /// could not render the template
    Render(#[from] askama::Error),
}

pub type DashboardResult<T> = Result<T, DashboardError>;

impl IntoResponse for DashboardError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}
