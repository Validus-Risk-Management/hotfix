use crate::AppState;
use crate::data_provider::DataProvider;
use axum::Router;
use axum::routing::get;

use crate::endpoints::health::get_health;
use crate::endpoints::session_info::get_session_info;

#[cfg(feature = "admin")]
mod admin;
mod health;
mod session_info;

#[cfg(feature = "admin")]
use admin::register_admin_endpoints;

pub fn build_api_router<P: DataProvider + 'static>() -> Router<AppState<P>> {
    let mut router = Router::new()
        .route("/health", get(get_health))
        .route("/session-info", get(get_session_info));
    router = register_admin_endpoints(router);

    router
}

#[cfg(not(feature = "admin"))]
fn register_admin_endpoints<P: DataProvider + 'static>(
    router: Router<AppState<P>>,
) -> Router<AppState<P>> {
    router
}
