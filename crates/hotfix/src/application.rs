use anyhow::anyhow;
use tokio::sync::mpsc;

use crate::message::FixMessage;

#[async_trait::async_trait]
/// The application users of HotFIX can implement to hook into the engine.
pub trait Application<M>: Send + Sync + 'static {
    /// Called when a message is sent to the engine to be sent to the counterparty.
    ///
    /// This is invoked before the raw message is persisted in the message store.
    async fn on_outbound_message(&self, msg: M);
    /// Called when a message is received from the counterparty.
    ///
    /// This is invoked after the message is verified and parsed into a typed message.
    async fn on_inbound_message(&self, msg: M);
    /// Called when the session is logged out.
    async fn on_logout(&mut self, reason: &str);
}

#[derive(Debug, Clone)]
enum ApplicationMessage<M> {
    #[allow(dead_code)]
    SendingMessage(M),
    ReceivedMessage(M),
    LoggedOut(String),
}

#[derive(Clone)]
pub struct ApplicationRef<M> {
    sender: mpsc::Sender<ApplicationMessage<M>>,
}

impl<M: FixMessage> ApplicationRef<M> {
    pub fn new(application: impl Application<M>) -> Self {
        let (sender, mailbox) = mpsc::channel::<ApplicationMessage<M>>(10);
        let actor = ApplicationActor::new(mailbox, application);
        tokio::spawn(run_application(actor));

        Self { sender }
    }

    pub async fn send_on_outbound_message(&self, msg: M) -> anyhow::Result<()> {
        self.send_message(ApplicationMessage::SendingMessage(msg))
            .await
    }

    pub async fn send_on_inbound_message(&self, msg: M) -> anyhow::Result<()> {
        self.send_message(ApplicationMessage::ReceivedMessage(msg))
            .await
    }

    async fn send_message(&self, msg: ApplicationMessage<M>) -> anyhow::Result<()> {
        self.sender
            .send(msg)
            .await
            .map_err(|_| anyhow!("failed to send message to app"))
    }

    pub async fn send_logout(&self, reason: String) {
        self.sender
            .send(ApplicationMessage::LoggedOut(reason))
            .await
            .expect("be able tell the app we have been logged out");
    }
}

struct ApplicationActor<M, A> {
    mailbox: mpsc::Receiver<ApplicationMessage<M>>,
    application: A,
}

impl<M, A> ApplicationActor<M, A>
where
    M: FixMessage,
    A: Application<M>,
{
    fn new(mailbox: mpsc::Receiver<ApplicationMessage<M>>, application: A) -> Self {
        Self {
            mailbox,
            application,
        }
    }

    async fn handle(&mut self, msg: ApplicationMessage<M>) {
        match msg {
            ApplicationMessage::SendingMessage(m) => {
                self.application.on_outbound_message(m).await;
            }
            ApplicationMessage::ReceivedMessage(m) => {
                self.application.on_inbound_message(m).await;
            }
            ApplicationMessage::LoggedOut(reason) => {
                self.application.on_logout(&reason).await;
            }
        }
    }
}

async fn run_application<M: FixMessage, A: Application<M>>(mut actor: ApplicationActor<M, A>) {
    while let Some(msg) = actor.mailbox.recv().await {
        actor.handle(msg).await;
    }
}
