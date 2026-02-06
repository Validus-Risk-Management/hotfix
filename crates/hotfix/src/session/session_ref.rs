use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tracing::debug;

use crate::config::SessionConfig;
use crate::message::{OutboundMessage, RawFixMessage};
use crate::session::Session;
use crate::session::admin_request::AdminRequest;
use crate::session::error::{SendError, SendOutcome, SessionCreationError};
use crate::session::event::{AwaitingActiveSessionResponse, SessionEvent};
use crate::store::MessageStore;
use crate::transport::writer::WriterRef;
use crate::{Application, session};

/// A request to send an outbound message, optionally with confirmation.
pub(crate) struct OutboundRequest<M> {
    pub message: M,
    pub confirm: Option<oneshot::Sender<Result<SendOutcome, SendError>>>,
}

#[derive(Clone)]
pub struct InternalSessionRef<Outbound> {
    pub(crate) event_sender: mpsc::Sender<SessionEvent>,
    pub(crate) outbound_message_sender: mpsc::Sender<OutboundRequest<Outbound>>,
    pub(crate) admin_request_sender: mpsc::Sender<AdminRequest>,
}

impl<Outbound: OutboundMessage> InternalSessionRef<Outbound> {
    pub fn new(
        config: SessionConfig,
        application: impl Application<Outbound = Outbound>,
        store: impl MessageStore + 'static,
    ) -> Result<Self, SessionCreationError> {
        let (event_sender, event_receiver) = mpsc::channel::<SessionEvent>(100);
        let (outbound_message_sender, outbound_message_receiver) =
            mpsc::channel::<OutboundRequest<Outbound>>(10);
        let (admin_request_sender, admin_request_receiver) = mpsc::channel::<AdminRequest>(10);
        let session = Session::new(config, application, store)?;
        tokio::spawn(session::run_session(
            session,
            event_receiver,
            outbound_message_receiver,
            admin_request_receiver,
        ));

        Ok(Self {
            event_sender,
            outbound_message_sender,
            admin_request_sender,
        })
    }

    pub async fn register_writer(&self, writer: WriterRef) -> Result<(), SessionGone> {
        self.event_sender
            .send(SessionEvent::Connected(writer))
            .await?;

        Ok(())
    }

    pub async fn new_fix_message_received(&self, msg: RawFixMessage) -> Result<(), SessionGone> {
        self.event_sender
            .send(SessionEvent::FixMessageReceived(msg))
            .await?;

        Ok(())
    }

    pub async fn disconnect(&self, reason: String) -> Result<(), SessionGone> {
        self.event_sender
            .send(SessionEvent::Disconnected(reason))
            .await?;

        Ok(())
    }

    pub async fn should_reconnect(&self) -> Result<bool, SessionGone> {
        let (sender, receiver) = oneshot::channel();
        self.event_sender
            .send(SessionEvent::ShouldReconnect(sender))
            .await?;
        Ok(receiver.await?)
    }

    pub async fn await_active_session_time(&self) -> Result<(), SessionGone> {
        debug!("awaiting active session time");
        let (sender, receiver) = oneshot::channel::<AwaitingActiveSessionResponse>();
        self.event_sender
            .send(SessionEvent::AwaitingActiveSession(sender))
            .await?;
        receiver.await?;

        debug!("resuming connection as session is active");
        Ok(())
    }
}

#[derive(Debug, Error)]
#[error("session task terminated")]
pub struct SessionGone(String);

impl From<mpsc::error::SendError<SessionEvent>> for SessionGone {
    fn from(err: mpsc::error::SendError<SessionEvent>) -> Self {
        Self(err.to_string())
    }
}

impl From<oneshot::error::RecvError> for SessionGone {
    fn from(err: oneshot::error::RecvError) -> Self {
        Self(err.to_string())
    }
}
