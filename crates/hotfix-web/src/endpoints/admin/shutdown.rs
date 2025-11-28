use crate::AppState;
use crate::error::AppResult;
use crate::session_controller::SessionController;
use axum::Json;
use axum::extract::State;
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct ShutdownRequest {
    pub reconnect: bool,
}

pub(crate) async fn shutdown<C: SessionController>(
    State(state): State<AppState<C>>,
    Json(payload): Json<ShutdownRequest>,
) -> AppResult<Json<()>> {
    state.controller.shutdown(payload.reconnect).await?;

    Ok(Json(()))
}
