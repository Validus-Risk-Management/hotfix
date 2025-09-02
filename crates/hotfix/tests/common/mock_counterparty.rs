use hotfix::config::SessionConfig;
use hotfix::message::logon::{Logon, ResetSeqNumConfig};
use hotfix::message::sequence_reset::SequenceReset;
use hotfix::message::{FixMessage, RawFixMessage, generate_message};
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

pub struct MockCounterparty<M> {
    receiver: Receiver<WriterMessage>,
    received_messages: Vec<Message>,
    sent_messages: Vec<Vec<u8>>,
    session_ref: SessionRef<M>,
    session_config: SessionConfig,
    dictionary: Dictionary,
    message_config: MessageConfig,
    _connection: FixConnection,
    _dc_sender: oneshot::Sender<()>,
}

impl<M> MockCounterparty<M>
where
    M: FixMessage,
{
    pub async fn start(session_ref: SessionRef<M>, session_config: SessionConfig) -> Self {
        let (writer_ref, receiver) = Self::create_writer();
        let (reader_ref, dc_sender) = Self::create_reader();
        let connection = FixConnection::new(writer_ref, reader_ref);

        session_ref.register_writer(connection.get_writer()).await;

        Self {
            receiver,
            received_messages: vec![],
            sent_messages: vec![],
            session_ref,
            session_config,
            dictionary: Dictionary::fix44(),
            message_config: MessageConfig::default(),
            _connection: connection,
            _dc_sender: dc_sender,
        }
    }

    pub async fn push_previously_sent_message(&mut self, message: impl FixMessage) {
        let raw_message = generate_message(
            &self.session_config.sender_comp_id,
            &self.session_config.target_comp_id,
            self.sent_messages.len() + 1,
            message,
        )
        .expect("failed to generate message");
        self.sent_messages.push(raw_message);
    }

    pub async fn resend_message(&mut self, sequence_number: u64) {
        let message = self.sent_messages[sequence_number as usize - 1].clone();
        self.session_ref
            .new_fix_message_received(RawFixMessage::new(message))
            .await;
    }

    pub async fn send_gap_fill(&mut self, start_seq_no: u64, new_seq_no: u64) {
        let sequence_reset = SequenceReset {
            gap_fill: true,
            new_seq_no,
        };
        let raw_message = generate_message(
            &self.session_config.sender_comp_id,
            &self.session_config.target_comp_id,
            start_seq_no as usize,
            sequence_reset,
        )
        .expect("failed to generate message");
        self.session_ref
            .new_fix_message_received(RawFixMessage::new(raw_message))
            .await;
    }

    pub async fn send_logon(&mut self) {
        let logon = Logon::new(
            self.session_config.heartbeat_interval,
            ResetSeqNumConfig::NoReset(None),
        );
        self.send_message(logon).await;
    }

    pub async fn send_message(&mut self, message: impl FixMessage) {
        let raw_message = generate_message(
            &self.session_config.sender_comp_id,
            &self.session_config.target_comp_id,
            self.sent_messages.len() + 1,
            message,
        )
        .expect("failed to generate message");
        self.sent_messages.push(raw_message.clone());
        self.session_ref
            .new_fix_message_received(RawFixMessage::new(raw_message))
            .await;
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
                    self.received_messages.push(message);
                    self.received_messages.last()
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

    pub(crate) async fn assert_next_with_timeout<F>(&mut self, assertion: F, timeout: Duration)
    where
        F: FnOnce(&Message),
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
