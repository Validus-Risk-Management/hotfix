use crate::AppState;
use crate::data_provider::DataProvider;
use crate::ui::assets::static_handler;
use crate::ui::dashboard::dashboard_handler;
use axum::Router;
use axum::routing::get;

mod assets;
mod dashboard;

pub fn builder_ui_router<P: DataProvider + 'static>() -> Router<AppState<P>> {
    Router::new()
        .route("/", get(dashboard_handler))
        .route("/static/{*file}", get(static_handler))
}
