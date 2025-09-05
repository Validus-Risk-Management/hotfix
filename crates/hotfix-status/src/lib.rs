mod api;
mod data_provider;
mod error;
#[cfg(feature = "ui")]
mod ui;

use crate::api::build_api_router;
use crate::data_provider::SessionDataProvider;
use axum::Router;
use hotfix::message::FixMessage;
use hotfix::session::SessionRef;

#[derive(Clone)]
struct AppState<P> {
    data_provider: P,
}

#[cfg(feature = "ui")]
pub fn build_router<M: FixMessage>(session_ref: SessionRef<M>) -> Router {
    let data_provider = SessionDataProvider { session_ref };
    let state = AppState { data_provider };
    Router::new()
        .nest("/api", build_api_router())
        .merge(ui::builder_ui_router())
        .with_state(state)
}

#[cfg(not(feature = "ui"))]
pub fn build_router<M: FixMessage>(session_ref: SessionRef<M>) -> Router {
    let data_provider = SessionDataProvider { session_ref };
    let state = AppState { data_provider };
    Router::new().nest("/api", build_api_router(state))
}
