use crate::session::admin_request::AdminRequest;
use crate::session::{InternalSessionRef, SessionInfo};
use anyhow::anyhow;
use tokio::sync::{mpsc, oneshot};

#[derive(Clone, Debug)]
pub struct SessionHandle<M> {
    outbound_message_sender: mpsc::Sender<M>,
    admin_request_sender: mpsc::Sender<AdminRequest>,
}

impl<M> SessionHandle<M> {
    pub async fn get_session_info(&self) -> anyhow::Result<SessionInfo> {
        let (sender, receiver) = oneshot::channel::<SessionInfo>();
        self.admin_request_sender
            .send(AdminRequest::RequestSessionInfo(sender))
            .await?;
        Ok(receiver.await?)
    }

    pub async fn send_message(&self, msg: M) -> anyhow::Result<()> {
        self.outbound_message_sender
            .send(msg)
            .await
            .map_err(|_| anyhow!("failed to send message"))?;

        Ok(())
    }

    pub async fn shutdown(&self, reconnect: bool) {
        self.admin_request_sender
            .send(AdminRequest::InitiateGracefulShutdown { reconnect })
            .await
            .unwrap();
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
