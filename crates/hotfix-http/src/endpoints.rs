use crate::AppState;
use crate::data_provider::DataProvider;
use axum::Router;
use axum::routing::get;

use crate::endpoints::health::get_health;
use crate::endpoints::session_info::get_session_info;

mod health;
mod session_info;

pub fn build_api_router<P: DataProvider + 'static>() -> Router<AppState<P>> {
    let mut router = Router::new()
        .route("/health", get(get_health))
        .route("/session-info", get(get_session_info));

    if cfg!(feature = "admin") {
        router = register_admin_endpoints(router);
    }

    router
}

fn register_admin_endpoints<P: DataProvider + 'static>(
    router: Router<AppState<P>>,
) -> Router<AppState<P>> {
    router
}
