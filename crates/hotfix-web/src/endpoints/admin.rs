use crate::AppState;
use crate::endpoints::admin::reset::reset_on_next_logon;
use crate::endpoints::admin::shutdown::shutdown;
use crate::session_controller::SessionController;
use axum::Router;
use axum::routing::post;

mod reset;
mod shutdown;

pub(crate) fn register_admin_endpoints<C: SessionController + 'static>(
    router: Router<AppState<C>>,
) -> Router<AppState<C>> {
    router
        .route("/shutdown", post(shutdown))
        .route("/reset", post(reset_on_next_logon))
}
