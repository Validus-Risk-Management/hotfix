mod api;
mod data_provider;
#[cfg(feature = "ui")]
mod ui;

use crate::api::build_api_router;
use axum::Router;
use hotfix::message::FixMessage;
use hotfix::session::SessionRef;

#[derive(Clone)]
struct AppState<P> {
    data_provider: P,
}

#[cfg(feature = "ui")]
pub fn build_router<M: FixMessage>(session_ref: SessionRef<M>) -> Router {
    Router::new()
        .nest("/api", build_api_router(session_ref))
        .merge(crate::ui::builder_ui_router())
}

#[cfg(not(feature = "ui"))]
pub fn build_router<M: FixMessage>(session_ref: SessionRef<M>) -> Router {
    Router::new().nest("/api", build_api_router(session_ref))
}
