use crate::config::SessionConfig;
use crate::message::{FixMessage, RawFixMessage};
use crate::session::event::{AwaitingActiveSessionResponse, SessionEvent};
use crate::session::{Session, SessionInfo};
use crate::store::MessageStore;
use crate::transport::writer::WriterRef;
use crate::{Application, session};
use tokio::sync::{mpsc, oneshot};
use tracing::debug;

#[derive(Clone)]
pub struct SessionRef<M> {
    sender: mpsc::Sender<SessionEvent<M>>,
}

impl<M: FixMessage> SessionRef<M> {
    pub fn new(
        config: SessionConfig,
        application: impl Application<M>,
        store: impl MessageStore + Send + Sync + 'static,
    ) -> Self {
        let (sender, mailbox) = mpsc::channel::<SessionEvent<M>>(10);
        let actor = Session::new(mailbox, config, application, store);
        tokio::spawn(session::run_session(actor));

        Self { sender }
    }

    pub async fn register_writer(&self, writer: WriterRef) {
        self.sender
            .send(SessionEvent::Connected(writer))
            .await
            .expect("be able to register writer");
    }

    pub async fn new_fix_message_received(&self, msg: RawFixMessage) {
        self.sender
            .send(SessionEvent::FixMessageReceived(msg))
            .await
            .expect("be able to receive message");
    }

    pub async fn disconnect(&self, reason: String) {
        self.sender
            .send(SessionEvent::Disconnected(reason))
            .await
            .expect("be able to send disconnect");
    }

    pub async fn send_message(&self, msg: M) {
        self.sender
            .send(SessionEvent::SendMessage(msg))
            .await
            .expect("message to send successfully");
    }

    pub async fn should_reconnect(&self) -> bool {
        let (sender, receiver) = oneshot::channel();
        self.sender
            .send(SessionEvent::ShouldReconnect(sender))
            .await
            .unwrap();
        receiver.await.expect("to receive a response")
    }

    pub async fn await_active_session_time(&self) {
        debug!("awaiting active session time");
        let (sender, receiver) = oneshot::channel::<AwaitingActiveSessionResponse>();
        self.sender
            .send(SessionEvent::AwaitingActiveSession(sender))
            .await
            .unwrap();
        receiver.await.expect("to receive a response");
        debug!("resuming connection as session is active");
    }

    pub async fn get_session_info(&self) -> SessionInfo {
        let (sender, receiver) = oneshot::channel::<SessionInfo>();
        self.sender
            .send(SessionEvent::SessionInfoRequested(sender))
            .await
            .unwrap();
        receiver.await.expect("to receive a response")
    }

    pub async fn shutdown(&self) {
        self.sender
            .send(SessionEvent::ShutdownRequested)
            .await
            .unwrap();
    }
}
