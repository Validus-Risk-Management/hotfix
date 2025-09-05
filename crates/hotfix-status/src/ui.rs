use crate::ui::assets::static_handler;
use crate::ui::dashboard::dashboard_handler;
use axum::Router;
use axum::routing::get;

mod assets;
mod dashboard;

pub fn builder_ui_router() -> Router {
    Router::new()
        .route("/dashboard", get(dashboard_handler))
        .route("/static/{*file}", get(static_handler))
}
