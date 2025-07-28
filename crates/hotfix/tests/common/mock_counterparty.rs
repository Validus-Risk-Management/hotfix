use hotfix::message::{FixMessage, RawFixMessage};
use hotfix::session::SessionRef;
use hotfix::transport::FixConnection;
use hotfix::transport::reader::ReaderRef;
use hotfix::transport::writer::{WriterMessage, WriterRef};
use hotfix_message::dict::Dictionary;
use hotfix_message::message::{Config as MessageConfig, Message};
use hotfix_message::parsed_message::ParsedMessage;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use tokio::sync::{mpsc, oneshot};

const DEFAULT_TIMEOUT: Duration = Duration::from_millis(10);

pub struct MockCounterparty {
    receiver: Receiver<WriterMessage>,
    // History of received messages from the session
    messages: Vec<Message>,
    dictionary: Dictionary,
    message_config: MessageConfig,
    _connection: FixConnection,
    _dc_sender: oneshot::Sender<()>,
}

impl MockCounterparty {
    pub async fn start(session_ref: SessionRef<impl FixMessage>) -> Self {
        let (writer_ref, receiver) = Self::create_writer();
        let (reader_ref, dc_sender) = Self::create_reader();
        let connection = FixConnection::new(writer_ref, reader_ref);

        session_ref.register_writer(connection.get_writer()).await;

        Self {
            receiver,
            messages: vec![],
            dictionary: Dictionary::fix44(),
            message_config: MessageConfig::default(),
            _connection: connection,
            _dc_sender: dc_sender,
        }
    }

    /// Waits for and returns the next message received from the session.
    ///
    /// A `None` response indicates we have been disconnected, either through the channel
    /// dropping on the session's side, or through an explicit `Disconnect` message.
    async fn get_next(&mut self) -> Option<&Message> {
        self.receiver
            .recv()
            .await
            .and_then(|writer_message| match writer_message {
                WriterMessage::SendMessage(raw_message) => {
                    let message = self.parse_message(&raw_message);
                    self.messages.push(message);
                    self.messages.last()
                }
                WriterMessage::Disconnect => None,
            })
    }

    fn parse_message(&self, raw_message: &RawFixMessage) -> Message {
        match Message::from_bytes(
            &self.message_config,
            &self.dictionary,
            raw_message.as_bytes(),
        ) {
            ParsedMessage::Valid(valid_message) => valid_message,
            _ => {
                panic!("only valid messages are supported in the mock counterparty")
            }
        }
    }

    pub async fn assert_next<F>(&mut self, assertion: F)
    where
        F: FnOnce(&Message) -> bool,
    {
        self.assert_next_with_timeout(assertion, DEFAULT_TIMEOUT)
            .await;
    }

    pub async fn assert_next_with_timeout<F>(&mut self, assertion: F, timeout: Duration)
    where
        F: FnOnce(&Message) -> bool,
    {
        match tokio::time::timeout(timeout, self.get_next()).await {
            Ok(Some(message)) => {
                assertion(message);
            }
            Ok(None) => {
                panic!("disconnected before receiving any message");
            }
            Err(_) => {
                panic!("timeout expired before receiving any message");
            }
        }
    }

    pub async fn assert_disconnected(&mut self) {
        self.assert_disconnected_with_timeout(DEFAULT_TIMEOUT).await;
    }

    pub async fn assert_disconnected_with_timeout(&mut self, timeout: Duration) {
        if tokio::time::timeout(timeout, async {
            // keep consuming messages until a disconnect occurs
            while self.get_next().await.is_some() {}
        })
        .await
        .is_err()
        {
            panic!("timeout expired before a disconnect occurred");
        }
    }

    fn create_writer() -> (WriterRef, Receiver<WriterMessage>) {
        let (sender, receiver) = mpsc::channel(10);
        (WriterRef::new(sender), receiver)
    }

    fn create_reader() -> (ReaderRef, oneshot::Sender<()>) {
        let (dc_sender, dc_receiver) = oneshot::channel();
        (ReaderRef::new(dc_receiver), dc_sender)
    }
}
