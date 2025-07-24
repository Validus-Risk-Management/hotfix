use hotfix::message::{FixMessage, RawFixMessage};
use hotfix::session::SessionRef;
use hotfix::transport::FixConnection;
use hotfix::transport::reader::ReaderRef;
use hotfix::transport::writer::{WriterMessage, WriterRef};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::Receiver;
use tokio::sync::{Mutex, mpsc, oneshot};

pub struct MockCounterparty {
    received_messages: Arc<Mutex<Vec<RawFixMessage>>>,
    _connection: FixConnection,
    _dc_sender: oneshot::Sender<()>,
}

type MessageStore = Arc<Mutex<Vec<RawFixMessage>>>;

impl MockCounterparty {
    pub async fn start(session_ref: SessionRef<impl FixMessage>) -> Self {
        let (writer_ref, received_messages) = Self::spawn_writer();
        let (reader_ref, dc_sender) = Self::create_reader();
        let connection = FixConnection::new(writer_ref, reader_ref);

        session_ref.register_writer(connection.get_writer()).await;

        Self {
            received_messages,
            _connection: connection,
            _dc_sender: dc_sender,
        }
    }

    pub async fn assert_message_count(&self, expected_count: usize, timeout_secs: f32) {
        let timeout_duration = Duration::from_secs_f32(timeout_secs);
        let start_time = Instant::now();

        loop {
            {
                let messages = self.received_messages.lock().await;
                if messages.len() >= expected_count {
                    return;
                }
            }

            if start_time.elapsed() >= timeout_duration {
                let current_count = self.received_messages.lock().await.len();
                panic!(
                    "Expected {expected_count} messages, but only received {current_count} within {timeout_secs} seconds"
                );
            }

            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    fn spawn_writer() -> (WriterRef, MessageStore) {
        // mock implementation to receive messages from the session
        // and hold on to them for test assertions
        let received_messages: MessageStore = Arc::new(Mutex::new(vec![]));
        let (sender, mailbox) = mpsc::channel(10);
        tokio::spawn(Self::run_writer(mailbox, received_messages.clone()));

        (WriterRef::new(sender), received_messages)
    }

    fn create_reader() -> (ReaderRef, oneshot::Sender<()>) {
        let (dc_sender, dc_receiver) = oneshot::channel();
        (ReaderRef::new(dc_receiver), dc_sender)
    }

    async fn run_writer(
        mut mailbox: Receiver<WriterMessage>,
        received_messages: Arc<Mutex<Vec<RawFixMessage>>>,
    ) {
        while let Some(msg) = mailbox.recv().await {
            match msg {
                WriterMessage::SendMessage(fix_message) => {
                    println!("Received message from session: {fix_message:?}");
                    received_messages.lock().await.push(fix_message);
                }
                WriterMessage::Disconnect => {
                    println!("Disconnecting");
                    break;
                }
            }
        }
    }
}
