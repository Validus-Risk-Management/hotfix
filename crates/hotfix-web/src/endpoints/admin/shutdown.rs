use crate::AppState;
use crate::error::AppResult;
use crate::session_controller::SessionController;
use axum::Json;
use axum::extract::State;

pub(crate) async fn shutdown<C: SessionController>(
    State(state): State<AppState<C>>,
) -> AppResult<Json<()>> {
    state.controller.shutdown(true).await?;

    Ok(Json(()))
}
