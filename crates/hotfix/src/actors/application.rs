use tokio::sync::mpsc;

use crate::message::FixMessage;

#[async_trait::async_trait]
pub trait Application<M>: Send + Sync + 'static {
    async fn on_message_from_app(&self, msg: M);
    async fn on_message_to_app(&self, msg: M);
    async fn on_logout(&mut self, reason: &str);
}

#[derive(Debug, Clone)]
pub enum ApplicationMessage<M> {
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

    pub async fn send_message(&self, msg: ApplicationMessage<M>) {
        self.sender
            .send(msg)
            .await
            .expect("be able to send message to app");
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
                self.application.on_message_from_app(m).await;
            }
            ApplicationMessage::ReceivedMessage(m) => {
                self.application.on_message_to_app(m).await;
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
