mod api;
mod data_provider;
mod ui;

use crate::api::build_api_router;
use crate::ui::builder_ui_router;
use axum::Router;
use hotfix::message::FixMessage;
use hotfix::session::SessionRef;

#[derive(Clone)]
struct AppState<P> {
    data_provider: P,
}

pub fn build_router<M: FixMessage>(session_ref: SessionRef<M>) -> Router {
    Router::new()
        .nest("/api", build_api_router(session_ref))
        .merge(builder_ui_router())
}
