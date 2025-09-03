use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

#[derive(Clone)]
struct AppState {}

pub fn build_router() -> Router {
    Router::new()
        .route("/health", get(get_health))
        .with_state(AppState {})
}

#[derive(Debug, Serialize)]
struct HealthStatus {
    status: String,
}

async fn get_health() -> Json<HealthStatus> {
    Json(HealthStatus {
        status: "healthy".to_string(),
    })
}
