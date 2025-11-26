mod assets;
mod dashboard;
mod error;

use axum::Router;
use axum::routing::get;
use hotfix::session::SessionInfo;

pub use error::{DashboardError, DashboardResult};

/// Trait for providing session data to the dashboard
#[async_trait::async_trait]
pub trait DashboardDataProvider: Clone + Send + Sync {
    async fn get_session_info(&self) -> anyhow::Result<SessionInfo>;
}

/// Build a router for the dashboard UI
///
/// This returns a router that works with any state `S` where you can
/// extract a `P: DataProvider` using `axum::extract::FromRef`.
///
/// Typically, your state will be a struct with a `data_provider` field.
pub fn build_ui_router<S, P>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    P: DashboardDataProvider + 'static,
    P: axum::extract::FromRef<S>,
{
    Router::new()
        .route("/", get(dashboard::dashboard_handler::<S, P>))
        .route("/static/{*file}", get(assets::static_handler))
}
