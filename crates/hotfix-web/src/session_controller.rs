use hotfix::message::FixMessage;
use hotfix::session::{SessionHandle, SessionInfo};

/// Controller for session operations, providing both read access and administrative actions
#[async_trait::async_trait]
pub trait SessionController: Clone + Send + Sync {
    async fn get_session_info(&self) -> anyhow::Result<SessionInfo>;
    async fn request_reset_on_next_logon(&self) -> anyhow::Result<()>;
    async fn shutdown(&self, reconnect: bool) -> anyhow::Result<()>;
}

/// HTTP session controller implementation that wraps a SessionHandle
#[derive(Clone)]
pub struct HttpSessionController<M> {
    pub(crate) session_handle: SessionHandle<M>,
}

#[async_trait::async_trait]
impl<M: FixMessage> SessionController for HttpSessionController<M> {
    async fn get_session_info(&self) -> anyhow::Result<SessionInfo> {
        self.session_handle.get_session_info().await
    }

    async fn request_reset_on_next_logon(&self) -> anyhow::Result<()> {
        self.session_handle.request_reset_on_next_logon().await
    }

    async fn shutdown(&self, reconnect: bool) -> anyhow::Result<()> {
        self.session_handle.shutdown(reconnect).await
    }
}

// Implement hotfix-web-ui's SessionInfoProvider for HttpSessionController
// Note: We can't use a blanket impl due to Rust's orphan rules (can't impl foreign trait for generic type)
#[cfg(feature = "ui")]
#[async_trait::async_trait]
impl<M: FixMessage> hotfix_web_ui::SessionInfoProvider for HttpSessionController<M> {
    async fn get_session_info(&self) -> anyhow::Result<SessionInfo> {
        // Reuse the SessionController implementation
        SessionController::get_session_info(self).await
    }
}

// Allow extracting HttpSessionController from AppState for hotfix-web-ui
#[cfg(feature = "ui")]
impl<M> axum::extract::FromRef<crate::AppState<HttpSessionController<M>>>
    for HttpSessionController<M>
where
    M: FixMessage,
{
    fn from_ref(state: &crate::AppState<HttpSessionController<M>>) -> Self {
        state.controller.clone()
    }
}
