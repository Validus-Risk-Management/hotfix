use crate::AppState;
use crate::data_provider::DataProvider;
use crate::error::AppResult;
use axum::Json;
use axum::extract::State;

pub(crate) async fn reset_on_next_logon<P: DataProvider>(
    State(state): State<AppState<P>>,
) -> AppResult<Json<()>> {
    state.data_provider.request_reset_on_next_logon().await?;

    Ok(Json(()))
}
