use crate::session::admin_request::AdminRequest;
use crate::session::{InternalSessionRef, SessionInfo};
use tokio::sync::{mpsc, oneshot};

#[derive(Clone, Debug)]
pub struct SessionHandle<M> {
    outbound_message_sender: mpsc::Sender<M>,
    admin_request_sender: mpsc::Sender<AdminRequest>,
}

impl<M> SessionHandle<M> {
    pub async fn get_session_info(&self) -> SessionInfo {
        let (sender, receiver) = oneshot::channel::<SessionInfo>();
        self.admin_request_sender
            .send(AdminRequest::RequestSessionInfo(sender))
            .await
            .unwrap();
        receiver.await.expect("to receive a response")
    }

    pub async fn send_message(&self, msg: M) {
        self.outbound_message_sender
            .send(msg)
            .await
            .expect("message to send successfully");
    }

    pub async fn shutdown(&self) {
        self.admin_request_sender
            .send(AdminRequest::RequestGracefulShutdown)
            .await
            .unwrap();
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
