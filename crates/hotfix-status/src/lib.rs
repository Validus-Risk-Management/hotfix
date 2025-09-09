mod api;
mod data_provider;
#[cfg(feature = "ui")]
mod error;
#[cfg(feature = "ui")]
mod ui;

use crate::api::build_api_router;
use crate::data_provider::{DataProvider, SessionDataProvider};
use axum::Router;
use hotfix::message::FixMessage;
use hotfix::session::SessionRef;

#[derive(Clone)]
struct AppState<P> {
    data_provider: P,
}

pub fn build_router<M: FixMessage>(session_ref: SessionRef<M>) -> Router {
    let data_provider = SessionDataProvider { session_ref };
    build_router_with_provider(data_provider)
}

#[cfg(feature = "ui")]
fn build_router_with_provider(data_provider: impl DataProvider + 'static) -> Router {
    let state = AppState { data_provider };
    Router::new()
        .nest("/api", build_api_router())
        .merge(ui::builder_ui_router())
        .with_state(state)
}

#[cfg(not(feature = "ui"))]
fn build_router_with_provider(data_provider: impl DataProvider + 'static) -> Router {
    let state = AppState { data_provider };
    Router::new()
        .nest("/api", build_api_router())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use crate::build_router_with_provider;
    use crate::data_provider::DataProvider;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use hotfix::session::{SessionInfo, Status};
    use serde_json::Value;
    use tower::Service;

    #[derive(Clone)]
    struct FakeDataProvider {
        session_info: SessionInfo,
    }

    #[async_trait::async_trait]
    impl DataProvider for FakeDataProvider {
        async fn get_session_info(&self) -> SessionInfo {
            self.session_info.clone()
        }
    }

    const DATA_PROVIDER: &FakeDataProvider = &FakeDataProvider {
        session_info: SessionInfo {
            next_sender_seq_number: 3,
            next_target_seq_number: 5,
            status: Status::AwaitingLogon,
        },
    };

    #[tokio::test]
    async fn test_get_health() {
        let mut router = build_router_with_provider(DATA_PROVIDER.clone());

        let response = router
            .call(Request::get("/api/health").body::<Body>("".into()).unwrap())
            .await
            .unwrap();

        assert_eq!(200, response.status());
    }

    #[tokio::test]
    async fn test_get_session_info() {
        let mut router = build_router_with_provider(DATA_PROVIDER.clone());

        let response = router
            .call(
                Request::get("/api/session-info")
                    .body::<Body>("".into())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(200, response.status());

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let parsed: Value = serde_json::from_slice(&body).unwrap();

        let next_sender_seq_number = parsed
            .get("session_info")
            .and_then(|session_info| session_info.get("next_sender_seq_number"))
            .and_then(|next_sender_seq_number| next_sender_seq_number.as_u64())
            .unwrap();
        assert_eq!(3, next_sender_seq_number);

        let next_target_seq_number = parsed
            .get("session_info")
            .and_then(|session_info| session_info.get("next_target_seq_number"))
            .and_then(|next_sender_seq_number| next_sender_seq_number.as_u64())
            .unwrap();
        assert_eq!(5, next_target_seq_number);

        let status = parsed
            .get("session_info")
            .and_then(|session_info| session_info.get("status"))
            .and_then(|status| status.as_str())
            .unwrap();
        assert_eq!("AwaitingLogon", status);
    }

    #[cfg(feature = "ui")]
    #[tokio::test]
    async fn test_get_dashboard() {
        let mut router = build_router_with_provider(DATA_PROVIDER.clone());
        let response = router
            .call(Request::get("/").body::<Body>("".into()).unwrap())
            .await
            .unwrap();

        assert_eq!(200, response.status());

        let headers = response.headers();
        assert_eq!(
            headers.get("content-type").unwrap(),
            "text/html; charset=utf-8"
        );
    }

    #[cfg(not(feature = "ui"))]
    #[tokio::test]
    async fn test_get_dashboard_without_ui_feature_returns_404() {
        let mut router = build_router_with_provider(DATA_PROVIDER.clone());
        let response = router
            .call(Request::get("/").body::<Body>("".into()).unwrap())
            .await
            .unwrap();

        assert_eq!(404, response.status());
    }

    #[cfg(feature = "ui")]
    #[tokio::test]
    async fn test_get_static_assets() {
        let mut router = build_router_with_provider(DATA_PROVIDER.clone());
        let response = router
            .call(
                Request::get("/static/tailwind.js")
                    .body::<Body>("".into())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(200, response.status());

        let headers = response.headers();
        let content_type = headers.get("content-type").unwrap().to_str().unwrap();
        assert!(
            content_type.contains("application/javascript")
                || content_type.contains("text/javascript")
                || content_type.contains("application/x-javascript")
        );
    }
}
