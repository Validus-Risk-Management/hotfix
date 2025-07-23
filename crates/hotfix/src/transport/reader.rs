use tokio::sync::oneshot;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ReaderMessage;

pub struct ReaderRef {
    disconnect_signal: oneshot::Receiver<()>,
}

impl ReaderRef {
    pub fn new(disconnect_signal: oneshot::Receiver<()>) -> Self {
        Self { disconnect_signal }
    }

    pub async fn wait_for_disconnect(self) {
        self.disconnect_signal
            .await
            .expect("not to drop signal prematurely");
    }
}
