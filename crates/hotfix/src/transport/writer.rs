use crate::message::parser::RawFixMessage;
use tokio::sync::mpsc;
use tracing::warn;

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
        if let Err(err) = self.sender.send(WriterMessage::SendMessage(msg)).await {
            // If the channel is closed, the writer task has terminated.
            // The session will receive a Disconnected event with the actual
            // disconnection reason, so we don't need to handle the error here.
            // The message we failed to send will be recovered by the counterparty
            // through the built-in recovery mechanisms of FIX.
            warn!("trying to send message but the writer is gone: {}", err);
        }
    }

    pub async fn disconnect(&self) {
        if let Err(err) = self.sender.send(WriterMessage::Disconnect).await {
            // If the channel is closed, we're already effectively disconnected.
            warn!("trying to send disconnect but the writer is gone: {}", err);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn send_raw_message_does_not_panic_when_channel_closed() {
        let (sender, receiver) = mpsc::channel(1);
        let writer = WriterRef::new(sender);
        drop(receiver);

        writer.send_raw_message(RawFixMessage::new(vec![])).await;
    }

    #[tokio::test]
    async fn disconnect_does_not_panic_when_channel_closed() {
        let (sender, receiver) = mpsc::channel(1);
        let writer = WriterRef::new(sender);
        drop(receiver);

        writer.disconnect().await;
    }
}
