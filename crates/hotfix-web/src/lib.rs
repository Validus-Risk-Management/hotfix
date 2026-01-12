mod endpoints;
mod error;
mod session_controller;

use crate::endpoints::build_api_router;
use crate::session_controller::{HttpSessionController, SessionController};
use axum::Router;
use hotfix::message::OutboundMessage;
use hotfix::session::SessionHandle;

#[derive(Clone)]
pub(crate) struct AppState<C> {
    pub(crate) controller: C,
}

/// Configuration for the HTTP router
#[derive(Clone, Debug, Default)]
pub struct RouterConfig {
    /// Enable admin endpoints (/api/shutdown, /api/reset)
    pub enable_admin_endpoints: bool,
}

/// Build a router with default configuration (admin endpoints disabled)
pub fn build_router<Outbound: OutboundMessage>(session_handle: SessionHandle<Outbound>) -> Router {
    build_router_with_config(session_handle, RouterConfig::default())
}

/// Build a router with custom configuration
pub fn build_router_with_config<Outbound: OutboundMessage>(
    session_handle: SessionHandle<Outbound>,
    config: RouterConfig,
) -> Router {
    let controller = HttpSessionController { session_handle };
    build_router_with_controller(controller, config)
}

#[cfg(feature = "ui")]
fn build_router_with_controller<C>(controller: C, config: RouterConfig) -> Router
where
    C: SessionController + hotfix_web_ui::SessionInfoProvider + 'static,
    C: axum::extract::FromRef<AppState<C>>,
{
    let state = AppState { controller };
    Router::new()
        .nest("/api", build_api_router(config))
        .merge(hotfix_web_ui::build_ui_router::<AppState<C>, C>())
        .with_state(state)
}

#[cfg(not(feature = "ui"))]
fn build_router_with_controller(
    controller: impl SessionController + 'static,
    config: RouterConfig,
) -> Router {
    let state = AppState { controller };
    Router::new()
        .nest("/api", build_api_router(config))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "ui")]
    use crate::AppState;
    use crate::RouterConfig;
    use crate::build_router_with_controller;
    use crate::session_controller::SessionController;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
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
    struct FakeSessionController {
        state: Arc<Mutex<FakeDataState>>,
    }

    impl FakeSessionController {
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
    impl SessionController for FakeSessionController {
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

    // Implement SessionInfoProvider for the test controller
    #[cfg(feature = "ui")]
    #[async_trait::async_trait]
    impl hotfix_web_ui::SessionInfoProvider for FakeSessionController {
        async fn get_session_info(&self) -> anyhow::Result<SessionInfo> {
            // Reuse the SessionController implementation
            SessionController::get_session_info(self).await
        }
    }

    // Allow extracting FakeSessionController from AppState for hotfix-web-ui
    #[cfg(feature = "ui")]
    impl axum::extract::FromRef<AppState<FakeSessionController>> for FakeSessionController {
        fn from_ref(state: &AppState<FakeSessionController>) -> Self {
            state.controller.clone()
        }
    }

    struct TestContext {
        router: Router,
        controller: FakeSessionController,
        config: RouterConfig,
    }

    impl TestContext {
        fn new() -> Self {
            Self::with_config(RouterConfig::default())
        }

        fn with_config(config: RouterConfig) -> Self {
            let controller = FakeSessionController::new();
            let router = build_router_with_controller(controller.clone(), config.clone());
            Self {
                router,
                controller,
                config,
            }
        }

        fn with_session_info(mut self, session_info: SessionInfo) -> Self {
            self.controller = self.controller.with_session_info(session_info);
            self.router =
                build_router_with_controller(self.controller.clone(), self.config.clone());
            self
        }

        async fn get(&mut self, path: &str) -> TestResponse {
            self.request(Method::GET, path).await
        }

        async fn post(&mut self, path: &str) -> TestResponse {
            self.request(Method::POST, path).await
        }

        async fn post_json(&mut self, path: &str, json: Value) -> TestResponse {
            let body = serde_json::to_string(&json).unwrap();
            let request = Request::builder()
                .method(Method::POST)
                .uri(path)
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap();

            let response = self.router.clone().oneshot(request).await.unwrap();
            TestResponse::new(response).await
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
            self.controller.get_state()
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
                self.status,
                expected,
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

    #[tokio::test]
    async fn test_reset_endpoint_triggers_reset_request() {
        let config = RouterConfig {
            enable_admin_endpoints: true,
        };
        let mut ctx = TestContext::with_config(config);

        let response = ctx.post("/api/reset").await;

        response.assert_status(StatusCode::OK);
        let state = ctx.get_state();
        assert!(state.reset_requested, "Reset should have been requested");
    }

    #[tokio::test]
    async fn test_shutdown_endpoint_calls_shutdown_with_reconnect() {
        let config = RouterConfig {
            enable_admin_endpoints: true,
        };
        let mut ctx = TestContext::with_config(config);

        let response = ctx
            .post_json("/api/shutdown", serde_json::json!({"reconnect": true}))
            .await;

        response.assert_status(StatusCode::OK);
        let state = ctx.get_state();
        assert!(state.shutdown_called, "Shutdown should have been called");
        assert_eq!(
            state.shutdown_reconnect,
            Some(true),
            "Shutdown should be called with reconnect=true"
        );
    }

    #[tokio::test]
    async fn test_shutdown_endpoint_calls_shutdown_without_reconnect() {
        let config = RouterConfig {
            enable_admin_endpoints: true,
        };
        let mut ctx = TestContext::with_config(config);

        let response = ctx
            .post_json("/api/shutdown", serde_json::json!({"reconnect": false}))
            .await;

        response.assert_status(StatusCode::OK);
        let state = ctx.get_state();
        assert!(state.shutdown_called, "Shutdown should have been called");
        assert_eq!(
            state.shutdown_reconnect,
            Some(false),
            "Shutdown should be called with reconnect=false"
        );
    }

    #[tokio::test]
    async fn test_admin_endpoints_disabled_by_default() {
        let mut ctx = TestContext::new(); // Default config has admin disabled

        let response = ctx.post("/api/reset").await;
        response.assert_status(StatusCode::NOT_FOUND);

        let response = ctx.post("/api/shutdown").await;
        response.assert_status(StatusCode::NOT_FOUND);
    }
}
