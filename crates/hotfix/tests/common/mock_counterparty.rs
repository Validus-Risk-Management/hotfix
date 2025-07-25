use hotfix::message::FixMessage;
use hotfix::session::SessionRef;
use hotfix::transport::FixConnection;
use hotfix::transport::reader::ReaderRef;
use hotfix::transport::writer::{WriterMessage, WriterRef};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(1);

pub struct MockCounterparty {
    // Receiver-End of the channel
    receiver: mpsc::UnboundedReceiver<WriterMessage>,
    // History of Received messages from the client
    messages: Vec<WriterMessage>,
    _connection: FixConnection,
    _dc_sender: oneshot::Sender<()>,
}

impl MockCounterparty {
    pub async fn start(session_ref: SessionRef<impl FixMessage>) -> Self {
        let (writer_ref, receiver) = Self::spawn_writer();
        let (reader_ref, dc_sender) = Self::create_reader();
        let connection = FixConnection::new(writer_ref, reader_ref);

        session_ref.register_writer(connection.get_writer()).await;

        Self {
            receiver,
            messages: vec![],
            _connection: connection,
            _dc_sender: dc_sender,
        }
    }

    /// Listen to the next message on the channel
    pub async fn get_next(&mut self, timeout: Option<Duration>) -> &WriterMessage {
        let timeout = timeout.unwrap_or(DEFAULT_TIMEOUT);
        let msg = tokio::time::timeout(timeout, self.receiver.recv())
            .await
            .unwrap_or_else(|_| panic!("Message not received in less than {timeout:?}"))
            .unwrap_or_else(|| panic!("Received message is None"));
        self.messages.push(msg);
        self.messages.last().unwrap()
    }

    pub async fn assert_next<F>(&mut self, timeout: Option<Duration>, assertion: F)
    where
        F: FnOnce(&WriterMessage) -> bool,
    {
        let msg = self.get_next(timeout).await;
        assert!(assertion(msg));
    }

    pub async fn assert_disconnected(&mut self, timeout: Option<Duration>) {
        self.assert_next(timeout, |msg| matches!(msg, &WriterMessage::Disconnect))
            .await;
        assert!(self.receiver.is_closed());
    }

    fn spawn_writer() -> (WriterRef, mpsc::UnboundedReceiver<WriterMessage>) {
        // mock implementation to receive messages from the session
        // and hold on to them for test assertions
        let (tx, rx) = mpsc::unbounded_channel();
        let (sender, mailbox) = mpsc::channel(10);
        tokio::spawn(Self::run_writer(mailbox, tx));

        (WriterRef::new(sender), rx)
    }

    fn create_reader() -> (ReaderRef, oneshot::Sender<()>) {
        let (dc_sender, dc_receiver) = oneshot::channel();
        (ReaderRef::new(dc_receiver), dc_sender)
    }

    async fn run_writer(
        mut mailbox: mpsc::Receiver<WriterMessage>,
        received_messages: mpsc::UnboundedSender<WriterMessage>,
    ) {
        while let Some(msg) = mailbox.recv().await {
            println!("Received message from session: {msg:?}");
            let disconnect = matches!(msg, WriterMessage::Disconnect);
            if let Err(e) = received_messages.send(msg) {
                panic!("Failed to send message. Error: {e:?}");
            }
            if disconnect {
                println!("Disconnecting");
                break;
            }
        }
    }
}
