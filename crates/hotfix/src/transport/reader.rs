use tokio::sync::oneshot;
use tracing::warn;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ReaderMessage;

pub struct ReaderRef {
    pub(crate) disconnect_signal: oneshot::Receiver<()>,
    pub(crate) kill: oneshot::Sender<()>,
}

impl ReaderRef {
    pub fn new(disconnect_signal: oneshot::Receiver<()>, kill: oneshot::Sender<()>) -> Self {
        Self {
            disconnect_signal,
            kill,
        }
    }

    pub async fn wait_for_disconnect(self) {
        if self.disconnect_signal.await.is_err() {
            warn!("reader dropped without issuing disconnect notification");
        }
    }
}
