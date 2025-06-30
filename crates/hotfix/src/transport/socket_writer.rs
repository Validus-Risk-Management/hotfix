use tokio::io::{AsyncWrite, AsyncWriteExt, WriteHalf};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::message::parser::RawFixMessage;

#[derive(Clone, Debug)]
pub enum WriterMessage {
    SendMessage(RawFixMessage),
    Disconnect,
}

#[derive(Clone, Debug)]
pub struct WriterRef {
    sender: mpsc::Sender<WriterMessage>,
}

impl WriterRef {
    pub fn new(writer: WriteHalf<impl AsyncWrite + Send + 'static>) -> Self {
        let (sender, mailbox) = mpsc::channel(10);
        let actor = WriterActor::new(writer, mailbox);
        tokio::spawn(run_writer(actor));

        Self { sender }
    }

    pub async fn send_raw_message(&self, msg: RawFixMessage) {
        self.sender
            .send(WriterMessage::SendMessage(msg))
            .await
            .expect("be able to send message");
    }

    pub async fn disconnect(&self) {
        self.sender
            .send(WriterMessage::Disconnect)
            .await
            .expect("be able to disconnect")
    }
}

struct WriterActor<W> {
    writer: WriteHalf<W>,
    mailbox: mpsc::Receiver<WriterMessage>,
}

impl<W: AsyncWrite> WriterActor<W> {
    fn new(writer: WriteHalf<W>, mailbox: mpsc::Receiver<WriterMessage>) -> Self {
        Self { writer, mailbox }
    }

    async fn handle(&mut self, message: WriterMessage) -> bool {
        match message {
            WriterMessage::SendMessage(fix_message) => {
                match self.writer.write_all(fix_message.as_bytes()).await {
                    Ok(_) => debug!("sent message: {}", fix_message),
                    // we don't shut down the writer due to errors, only when explicitly requested
                    // a broken connection is shut down via the reader -> session -> writer route
                    Err(_) => warn!("failed to send message: {}", fix_message),
                }
                true
            }
            WriterMessage::Disconnect => false,
        }
    }
}

async fn run_writer<W: AsyncWrite>(mut actor: WriterActor<W>) {
    while let Some(msg) = actor.mailbox.recv().await {
        if !actor.handle(msg).await {
            break;
        }
    }

    debug!("writer loop is shutting down");
}
