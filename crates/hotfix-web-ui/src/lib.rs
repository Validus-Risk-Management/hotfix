mod assets;
mod dashboard;
mod error;

use axum::Router;
use axum::routing::get;
use hotfix::session::SessionInfo;

pub use error::{DashboardError, DashboardResult};

/// Trait for providing session information to the dashboard
///
/// This is a read-only subset focused on displaying session data.
/// For full session control including admin actions, see the SessionController trait in hotfix-http.
#[async_trait::async_trait]
pub trait SessionInfoProvider: Clone + Send + Sync {
    async fn get_session_info(&self) -> anyhow::Result<SessionInfo>;
}

/// Build a router for the dashboard UI
///
/// This returns a router that works with any state `S` where you can
/// extract a `P: SessionInfoProvider` using `axum::extract::FromRef`.
///
/// Typically, your state will be a struct with a `controller` field.
pub fn build_ui_router<S, P>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    P: SessionInfoProvider + 'static,
    P: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/", get(dashboard::dashboard_handler::<S, P>))
        .route("/static/{*file}", get(assets::static_handler))
}
