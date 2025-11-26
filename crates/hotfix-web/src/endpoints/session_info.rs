use crate::AppState;
use crate::error::AppResult;
use crate::session_controller::SessionController;
use axum::Json;
use axum::extract::State;
use hotfix::session::SessionInfo;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SessionInfoResponse {
    session_info: SessionInfo,
}

pub(crate) async fn get_session_info<C: SessionController>(
    State(state): State<AppState<C>>,
) -> AppResult<Json<SessionInfoResponse>> {
    let session_info = state.controller.get_session_info().await?;

    Ok(Json(SessionInfoResponse { session_info }))
}
