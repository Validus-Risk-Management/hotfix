use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HealthStatusResponse {
    status: String,
}

pub(crate) async fn get_health() -> Json<HealthStatusResponse> {
    Json(HealthStatusResponse {
        status: "healthy".to_string(),
    })
}
