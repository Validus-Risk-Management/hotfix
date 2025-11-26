use crate::AppState;
use crate::data_provider::DataProvider;
use crate::endpoints::admin::reset::reset_on_next_logon;
use crate::endpoints::admin::shutdown::shutdown;
use axum::Router;
use axum::routing::post;

mod reset;
mod shutdown;

pub(crate) fn register_admin_endpoints<P: DataProvider + 'static>(
    router: Router<AppState<P>>,
) -> Router<AppState<P>> {
    router
        .route("/shutdown", post(shutdown))
        .route("/reset", post(reset_on_next_logon))
}
