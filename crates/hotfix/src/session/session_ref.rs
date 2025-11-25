use crate::config::SessionConfig;
use crate::message::{FixMessage, RawFixMessage};
use crate::session::Session;
use crate::session::admin_request::AdminRequest;
use crate::session::event::{AwaitingActiveSessionResponse, SessionEvent};
use crate::store::MessageStore;
use crate::transport::writer::WriterRef;
use crate::{Application, session};
use tokio::sync::{mpsc, oneshot};
use tracing::debug;

#[derive(Clone)]
pub struct InternalSessionRef<M> {
    pub(crate) event_sender: mpsc::Sender<SessionEvent>,
    pub(crate) outbound_message_sender: mpsc::Sender<M>,
    pub(crate) admin_request_sender: mpsc::Sender<AdminRequest>,
}

impl<M: FixMessage> InternalSessionRef<M> {
    pub fn new(
        config: SessionConfig,
        application: impl Application<M>,
        store: impl MessageStore + Send + Sync + 'static,
    ) -> Self {
        let (event_sender, event_receiver) = mpsc::channel::<SessionEvent>(100);
        let (outbound_message_sender, outbound_message_receiver) = mpsc::channel::<M>(10);
        let (admin_request_sender, admin_request_receiver) = mpsc::channel::<AdminRequest>(10);
        let session = Session::new(config, application, store);
        tokio::spawn(session::run_session(
            session,
            event_receiver,
            outbound_message_receiver,
            admin_request_receiver,
        ));

        Self {
            event_sender,
            outbound_message_sender,
            admin_request_sender,
        }
    }

    pub async fn register_writer(&self, writer: WriterRef) {
        self.event_sender
            .send(SessionEvent::Connected(writer))
            .await
            .expect("be able to register writer");
    }

    pub async fn new_fix_message_received(&self, msg: RawFixMessage) {
        self.event_sender
            .send(SessionEvent::FixMessageReceived(msg))
            .await
            .expect("be able to receive message");
    }

    pub async fn disconnect(&self, reason: String) {
        self.event_sender
            .send(SessionEvent::Disconnected(reason))
            .await
            .expect("be able to send disconnect");
    }

    pub async fn should_reconnect(&self) -> bool {
        let (sender, receiver) = oneshot::channel();
        self.event_sender
            .send(SessionEvent::ShouldReconnect(sender))
            .await
            .unwrap();
        receiver.await.expect("to receive a response")
    }

    pub async fn await_active_session_time(&self) {
        debug!("awaiting active session time");
        let (sender, receiver) = oneshot::channel::<AwaitingActiveSessionResponse>();
        self.event_sender
            .send(SessionEvent::AwaitingActiveSession(sender))
            .await
            .unwrap();
        receiver.await.expect("to receive a response");
        debug!("resuming connection as session is active");
    }
}
