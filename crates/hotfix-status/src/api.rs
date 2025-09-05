use crate::AppState;
use crate::data_provider::DataProvider;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use hotfix::session::SessionInfo;
use serde::Serialize;

pub fn build_api_router<P: DataProvider + 'static>() -> Router<AppState<P>> {
    Router::new()
        .route("/health", get(get_health))
        .route("/session-info", get(get_session_info))
}

#[derive(Debug, Serialize)]
struct HealthStatusResponse {
    status: String,
}

async fn get_health() -> Json<HealthStatusResponse> {
    Json(HealthStatusResponse {
        status: "healthy".to_string(),
    })
}

#[derive(Debug, Serialize)]
struct SessionInfoResponse {
    session_info: SessionInfo,
}

async fn get_session_info<P: DataProvider>(
    State(state): State<AppState<P>>,
) -> Json<SessionInfoResponse> {
    let session_info = state.data_provider.get_session_info().await;

    Json(SessionInfoResponse { session_info })
}
