use crate::AppState;
use crate::error::AppResult;
use crate::session_controller::SessionController;
use axum::Json;
use axum::extract::State;

pub(crate) async fn reset_on_next_logon<C: SessionController>(
    State(state): State<AppState<C>>,
) -> AppResult<Json<()>> {
    state.controller.request_reset_on_next_logon().await?;

    Ok(Json(()))
}
