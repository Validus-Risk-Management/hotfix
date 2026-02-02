use crate::session::admin_request::AdminRequest;
use crate::session::error::{SendError, SendOutcome};
use crate::session::session_ref::OutboundRequest;
use crate::session::{InternalSessionRef, SessionInfo};
use tokio::sync::{mpsc, oneshot};

/// A public handle to the session that can be used to interact with the session.
///
/// This wraps a subset of the channels of [`InternalSessionRef`].
/// Whilst [`InternalSessionRef`] is intended for internal use within the engine,
/// such as inbound message processing and disconnects, [`SessionHandle`] is public
/// and only exposes APIs intended for consumers of the engine.
#[derive(Clone, Debug)]
pub struct SessionHandle<Outbound> {
    outbound_message_sender: mpsc::Sender<OutboundRequest<Outbound>>,
    admin_request_sender: mpsc::Sender<AdminRequest>,
}

impl<Outbound> SessionHandle<Outbound> {
    pub async fn get_session_info(&self) -> anyhow::Result<SessionInfo> {
        let (sender, receiver) = oneshot::channel::<SessionInfo>();
        self.admin_request_sender
            .send(AdminRequest::RequestSessionInfo(sender))
            .await?;
        Ok(receiver.await?)
    }

    /// Sends a message and waits for confirmation that it was persisted.
    ///
    /// Returns `SendOutcome::Sent` with the sequence number if the message was
    /// successfully persisted and sent, or `SendOutcome::Dropped` if the application
    /// callback chose to drop the message.
    pub async fn send(&self, msg: Outbound) -> Result<SendOutcome, SendError> {
        let (tx, rx) = oneshot::channel();
        let request = OutboundRequest {
            message: msg,
            confirm: Some(tx),
        };
        self.outbound_message_sender
            .send(request)
            .await
            .map_err(|_| SendError::Disconnected)?;

        rx.await.map_err(|_| SendError::ConfirmationLost)?
    }

    /// Sends a message without waiting for confirmation.
    ///
    /// This is a fire-and-forget operation. The message will be queued for sending
    /// but no confirmation is provided about whether it was actually sent.
    pub async fn send_forget(&self, msg: Outbound) -> Result<(), SendError> {
        let request = OutboundRequest {
            message: msg,
            confirm: None,
        };
        self.outbound_message_sender
            .send(request)
            .await
            .map_err(|_| SendError::Disconnected)?;

        Ok(())
    }

    pub async fn shutdown(&self, reconnect: bool) -> anyhow::Result<()> {
        self.admin_request_sender
            .send(AdminRequest::InitiateGracefulShutdown { reconnect })
            .await?;

        Ok(())
    }

    pub async fn request_reset_on_next_logon(&self) -> anyhow::Result<()> {
        self.admin_request_sender
            .send(AdminRequest::ResetSequenceNumbersOnNextLogon)
            .await?;

        Ok(())
    }
}

impl<M> From<InternalSessionRef<M>> for SessionHandle<M> {
    fn from(session_ref: InternalSessionRef<M>) -> Self {
        Self {
            outbound_message_sender: session_ref.outbound_message_sender.clone(),
            admin_request_sender: session_ref.admin_request_sender.clone(),
        }
    }
}
