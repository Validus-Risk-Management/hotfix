use crate::{AppState, RouterConfig};
use crate::data_provider::DataProvider;
use axum::Router;
use axum::routing::get;

use crate::endpoints::health::get_health;
use crate::endpoints::session_info::get_session_info;

mod admin;
mod health;
mod session_info;

use admin::register_admin_endpoints;

pub fn build_api_router<P: DataProvider + 'static>(config: RouterConfig) -> Router<AppState<P>> {
    let mut router = Router::new()
        .route("/health", get(get_health))
        .route("/session-info", get(get_session_info));

    if config.enable_admin_endpoints {
        router = register_admin_endpoints(router);
    }

    router
}
