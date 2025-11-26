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
/// This requires router state that can serve the required data
/// to the endpoints as defined in [`SessionInfoProvider`].
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use hotfix::session::{SessionInfo, Status};
    use tower::ServiceExt;

    #[derive(Clone)]
    struct MockSessionProvider {
        session_info: SessionInfo,
    }

    #[async_trait::async_trait]
    impl SessionInfoProvider for MockSessionProvider {
        async fn get_session_info(&self) -> anyhow::Result<SessionInfo> {
            Ok(self.session_info.clone())
        }
    }

    fn create_test_app() -> Router {
        let provider = MockSessionProvider {
            session_info: SessionInfo {
                next_sender_seq_number: 42,
                next_target_seq_number: 100,
                status: Status::Active,
            },
        };
        build_ui_router::<MockSessionProvider, MockSessionProvider>().with_state(provider)
    }

    #[tokio::test]
    async fn test_dashboard_returns_html() {
        let app = create_test_app();

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());
        assert!(
            content_type.is_some_and(|ct| ct.contains("text/html")),
            "Expected HTML content type"
        );
    }

    #[tokio::test]
    async fn test_static_asset_returns_file() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/static/tailwind.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());
        assert!(
            content_type.is_some_and(
                |ct| ct.contains("javascript") || ct.contains("application/x-javascript")
            ),
            "Expected JavaScript content type, got: {:?}",
            content_type
        );
    }

    #[tokio::test]
    async fn test_static_asset_not_found() {
        let app = create_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/static/nonexistent.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
