use hotfix::message::FixMessage;
use hotfix::session::{SessionInfo, SessionRef};

#[async_trait::async_trait]
pub trait DataProvider: Clone + Send + Sync {
    async fn get_session_info(&self) -> SessionInfo;
}

#[derive(Clone)]
pub struct SessionDataProvider<M> {
    pub(crate) session_ref: SessionRef<M>,
}

#[async_trait::async_trait]
impl<M: FixMessage> DataProvider for SessionDataProvider<M> {
    async fn get_session_info(&self) -> SessionInfo {
        self.session_ref.get_session_info().await
    }
}
