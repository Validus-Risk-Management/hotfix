mod data_provider;
mod endpoints;
mod error;
#[cfg(feature = "ui")]
mod ui;

use crate::data_provider::{DataProvider, SessionDataProvider};
use crate::endpoints::build_api_router;
use axum::Router;
use hotfix::message::FixMessage;
use hotfix::session::SessionHandle;

#[derive(Clone)]
struct AppState<P> {
    data_provider: P,
}

pub fn build_router<M: FixMessage>(session_handle: SessionHandle<M>) -> Router {
    let data_provider = SessionDataProvider { session_handle };
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
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use axum::Router;
    use hotfix::session::{SessionInfo, Status};
    use serde_json::Value;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    #[derive(Clone, Debug)]
    struct FakeDataState {
        session_info: SessionInfo,
        reset_requested: bool,
        shutdown_called: bool,
        shutdown_reconnect: Option<bool>,
    }

    impl Default for FakeDataState {
        fn default() -> Self {
            Self {
                session_info: SessionInfo {
                    next_sender_seq_number: 3,
                    next_target_seq_number: 5,
                    status: Status::AwaitingLogon,
                },
                reset_requested: false,
                shutdown_called: false,
                shutdown_reconnect: None,
            }
        }
    }

    #[derive(Clone)]
    struct FakeDataProvider {
        state: Arc<Mutex<FakeDataState>>,
    }

    impl FakeDataProvider {
        fn new() -> Self {
            Self {
                state: Arc::new(Mutex::new(FakeDataState::default())),
            }
        }

        fn with_session_info(self, session_info: SessionInfo) -> Self {
            self.state.lock().unwrap().session_info = session_info;
            self
        }

        fn get_state(&self) -> FakeDataState {
            self.state.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl DataProvider for FakeDataProvider {
        async fn get_session_info(&self) -> anyhow::Result<SessionInfo> {
            let state = self.state.lock().unwrap();
            Ok(state.session_info.clone())
        }

        async fn request_reset_on_next_logon(&self) -> anyhow::Result<()> {
            let mut state = self.state.lock().unwrap();
            state.reset_requested = true;
            Ok(())
        }

        async fn shutdown(&self, reconnect: bool) -> anyhow::Result<()> {
            let mut state = self.state.lock().unwrap();
            state.shutdown_called = true;
            state.shutdown_reconnect = Some(reconnect);
            Ok(())
        }
    }

    struct TestContext {
        router: Router,
        data_provider: FakeDataProvider,
    }

    impl TestContext {
        fn new() -> Self {
            let data_provider = FakeDataProvider::new();
            let router = build_router_with_provider(data_provider.clone());
            Self {
                router,
                data_provider,
            }
        }

        fn with_session_info(mut self, session_info: SessionInfo) -> Self {
            self.data_provider = self.data_provider.with_session_info(session_info);
            self.router = build_router_with_provider(self.data_provider.clone());
            self
        }

        async fn get(&mut self, path: &str) -> TestResponse {
            self.request(Method::GET, path).await
        }

        async fn post(&mut self, path: &str) -> TestResponse {
            self.request(Method::POST, path).await
        }

        async fn request(&mut self, method: Method, path: &str) -> TestResponse {
            let request = Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .unwrap();

            let response = self.router.clone().oneshot(request).await.unwrap();
            TestResponse::new(response).await
        }

        fn get_state(&self) -> FakeDataState {
            self.data_provider.get_state()
        }
    }

    struct TestResponse {
        status: StatusCode,
        body: Vec<u8>,
    }

    impl TestResponse {
        async fn new(response: axum::response::Response) -> Self {
            let status = response.status();
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec();
            Self { status, body }
        }

        fn assert_status(&self, expected: StatusCode) -> &Self {
            assert_eq!(
                self.status, expected,
                "Expected status {}, got {}. Body: {}",
                expected,
                self.status,
                String::from_utf8_lossy(&self.body)
            );
            self
        }

        fn json_body(&self) -> Value {
            serde_json::from_slice(&self.body).unwrap()
        }
    }

    #[tokio::test]
    async fn test_health_endpoint_returns_healthy_status() {
        let mut ctx = TestContext::new();

        let response = ctx.get("/api/health").await;

        response.assert_status(StatusCode::OK);
        let body = response.json_body();
        assert_eq!(body["status"], "healthy");
    }

    #[tokio::test]
    async fn test_session_info_endpoint_returns_session_data() {
        let session_info = SessionInfo {
            next_sender_seq_number: 42,
            next_target_seq_number: 99,
            status: Status::Active,
        };

        let mut ctx = TestContext::new().with_session_info(session_info);

        let response = ctx.get("/api/session-info").await;

        response.assert_status(StatusCode::OK);
        let body = response.json_body();
        assert_eq!(body["session_info"]["next_sender_seq_number"], 42);
        assert_eq!(body["session_info"]["next_target_seq_number"], 99);
        assert_eq!(body["session_info"]["status"], "Active");
    }

    #[tokio::test]
    async fn test_session_info_with_awaiting_logon_status() {
        let session_info = SessionInfo {
            next_sender_seq_number: 1,
            next_target_seq_number: 1,
            status: Status::AwaitingLogon,
        };

        let mut ctx = TestContext::new().with_session_info(session_info);

        let response = ctx.get("/api/session-info").await;

        response.assert_status(StatusCode::OK);
        let body = response.json_body();
        assert_eq!(body["session_info"]["status"], "AwaitingLogon");
    }

    #[cfg(feature = "admin")]
    #[tokio::test]
    async fn test_reset_endpoint_triggers_reset_request() {
        let mut ctx = TestContext::new();

        let response = ctx.post("/api/reset").await;

        response.assert_status(StatusCode::OK);
        let state = ctx.get_state();
        assert!(state.reset_requested, "Reset should have been requested");
    }

    #[cfg(feature = "admin")]
    #[tokio::test]
    async fn test_shutdown_endpoint_calls_shutdown_with_reconnect() {
        let mut ctx = TestContext::new();

        let response = ctx.post("/api/shutdown").await;

        response.assert_status(StatusCode::OK);
        let state = ctx.get_state();
        assert!(state.shutdown_called, "Shutdown should have been called");
        assert_eq!(
            state.shutdown_reconnect,
            Some(true),
            "Shutdown should be called with reconnect=true"
        );
    }
}
