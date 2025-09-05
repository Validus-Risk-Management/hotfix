use hotfix::message::FixMessage;
use hotfix::session::{SessionInfo, SessionRef};

pub trait DataProvider {
    async fn get_session_info(&self) -> SessionInfo;
}

#[derive(Clone)]
pub struct SessionDataProvider<M> {
    pub(crate) session_ref: SessionRef<M>,
}

impl<M: FixMessage> DataProvider for SessionDataProvider<M> {
    async fn get_session_info(&self) -> SessionInfo {
        self.session_ref.get_session_info().await
    }
}
