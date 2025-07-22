use crate::message::parser::RawFixMessage;
use tokio::sync::mpsc;

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
    pub fn new(sender: mpsc::Sender<WriterMessage>) -> Self {
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
