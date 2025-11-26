use hotfix::message::FixMessage;
use hotfix::session::{SessionHandle, SessionInfo};

#[async_trait::async_trait]
pub trait DataProvider: Clone + Send + Sync {
    async fn get_session_info(&self) -> anyhow::Result<SessionInfo>;
    async fn request_reset_on_next_logon(&self) -> anyhow::Result<()>;
    async fn shutdown(&self, reconnect: bool) -> anyhow::Result<()>;
}

#[derive(Clone)]
pub struct SessionDataProvider<M> {
    pub(crate) session_handle: SessionHandle<M>,
}

#[async_trait::async_trait]
impl<M: FixMessage> DataProvider for SessionDataProvider<M> {
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
