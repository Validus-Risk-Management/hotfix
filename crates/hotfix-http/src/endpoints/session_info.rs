use crate::AppState;
use crate::data_provider::DataProvider;
use crate::error::AppResult;
use axum::Json;
use axum::extract::State;
use hotfix::session::SessionInfo;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SessionInfoResponse {
    session_info: SessionInfo,
}

pub(crate) async fn get_session_info<P: DataProvider>(
    State(state): State<AppState<P>>,
) -> AppResult<Json<SessionInfoResponse>> {
    let session_info = state.data_provider.get_session_info().await?;

    Ok(Json(SessionInfoResponse { session_info }))
}
