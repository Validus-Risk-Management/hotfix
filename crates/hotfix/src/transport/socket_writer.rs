use async_trait::async_trait;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::message::parser::RawFixMessage;
use crate::transport::actor::{Actor, WriterModel};

#[derive(Clone, Debug)]
pub enum WriterMessage {
    SendMessage(RawFixMessage),
    Disconnect,
}

#[derive(Clone, Debug)]
pub struct WriterRef {
    sender: mpsc::Sender<WriterMessage>,
}

pub struct WriterActor<W: AsyncWrite> {
    writer: W,
    mailbox: mpsc::Receiver<WriterMessage>,
}

impl<W: AsyncWrite> WriterActor<W> {
    fn new(writer: W, mailbox: mpsc::Receiver<WriterMessage>) -> Self {
        Self { writer, mailbox }
    }
}

#[async_trait]
impl<W: AsyncWrite + Send + Unpin + 'static> Actor<WriterMessage> for WriterActor<W> {
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
    async fn next(&mut self) -> Option<WriterMessage> {
        self.mailbox.recv().await
    }
}

#[async_trait]
impl WriterModel for WriterRef {
    fn new<W: AsyncWrite + Send + Unpin + 'static>(writer: W) -> Self {
        let (sender, mailbox) = mpsc::channel(10);
        let actor = WriterActor::new(writer, mailbox);
        actor.run();

        Self { sender }
    }

    async fn send_raw_message(&self, msg: RawFixMessage) {
        self.sender
            .send(WriterMessage::SendMessage(msg))
            .await
            .expect("be able to send message");
    }

    async fn disconnect(&self) {
        self.sender
            .send(WriterMessage::Disconnect)
            .await
            .expect("be able to disconnect")
    }
}

#[cfg(test)]
mod tests {
    use super::WriterRef;
    use crate::message::parser::RawFixMessage;
    use crate::transport::actor::WriterModel;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn test_mocked_writer() {
        let (writer, mut reader) = tokio::io::duplex(10);
        let writer_ref = WriterRef::new(writer);

        writer_ref
            .send_raw_message(RawFixMessage::new(vec![1, 2, 3]))
            .await;

        assert_eq!(1, reader.read_u8().await.unwrap());
        assert_eq!(2, reader.read_u8().await.unwrap());
        assert_eq!(3, reader.read_u8().await.unwrap());
    }
}
