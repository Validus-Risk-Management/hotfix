use crate::AppState;
use crate::data_provider::DataProvider;
use crate::error::AppResult;
use axum::Json;
use axum::extract::State;

pub(crate) async fn shutdown<P: DataProvider>(
    State(state): State<AppState<P>>,
) -> AppResult<Json<()>> {
    state.data_provider.shutdown(true).await?;

    Ok(Json(()))
}
