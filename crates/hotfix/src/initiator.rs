//! FIX initiator implementation.
//!
//! The fIX session that initiates the connection with its peer,
//! the acceptor. Currently, `HotFIX` only supports initiators.
//!
//! The initiator establishes the transport layer connection with
//! the peer, and sends the initial Logon (35=A) message. For transport,
//! `HotFIX` supports plain TCP and encrypted TLS over TCP connections.
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::application::Application;
use crate::config::SessionConfig;
use crate::message::OutboundMessage;
use crate::session::error::{SendError, SendOutcome, SessionCreationError};
use crate::session::{InternalSessionRef, SessionHandle};
use crate::store::MessageStore;
use crate::transport::connect;

#[derive(Clone)]
pub struct Initiator<Outbound> {
    pub config: SessionConfig,
    session_handle: SessionHandle<Outbound>,
    completion_rx: watch::Receiver<bool>,
}

impl<Outbound: OutboundMessage> Initiator<Outbound> {
    pub async fn start(
        config: SessionConfig,
        application: impl Application<Outbound = Outbound>,
        store: impl MessageStore + 'static,
    ) -> Result<Self, SessionCreationError> {
        let session_ref = InternalSessionRef::new(config.clone(), application, store)?;
        let (completion_tx, completion_rx) = watch::channel(false);

        tokio::spawn({
            let config = config.clone();
            let session_ref = session_ref.clone();
            establish_connection(config, session_ref, completion_tx)
        });

        let initiator = Self {
            config,
            session_handle: session_ref.into(),
            completion_rx,
        };

        Ok(initiator)
    }

    /// Sends a message and waits for confirmation that it was persisted.
    ///
    /// Returns `SendOutcome::Sent` with the sequence number if the message was
    /// successfully persisted and sent, or `SendOutcome::Dropped` if the application
    /// callback chose to drop the message.
    pub async fn send(&self, msg: Outbound) -> Result<SendOutcome, SendError> {
        self.session_handle.send(msg).await
    }

    /// Sends a message without waiting for confirmation.
    ///
    /// This is a fire-and-forget operation. The message will be queued for sending
    /// but no confirmation is provided about whether it was actually sent.
    pub async fn send_forget(&self, msg: Outbound) -> Result<(), SendError> {
        self.session_handle.send_forget(msg).await
    }

    pub fn is_interested(&self, sender_comp_id: &str, target_comp_id: &str) -> bool {
        self.config.sender_comp_id == sender_comp_id && self.config.target_comp_id == target_comp_id
    }

    pub fn session_handle(&self) -> SessionHandle<Outbound> {
        self.session_handle.clone()
    }

    pub async fn shutdown(self, reconnect: bool) -> Result<(), SendError> {
        self.session_handle.shutdown(reconnect).await?;
        tokio::time::timeout(Duration::from_secs(5), self.wait_for_shutdown())
            .await
            .map_err(|_| SendError::SessionGone)?;

        Ok(())
    }

    pub async fn wait_for_shutdown(&self) {
        let mut rx = self.completion_rx.clone();
        loop {
            if *rx.borrow_and_update() {
                break;
            } else if let Err(err) = rx.changed().await {
                warn!("connection loop has exited but completion signal was not sent: {err}");
                break;
            };
        }
    }

    pub fn is_shutdown(&self) -> bool {
        *self.completion_rx.borrow()
    }
}

async fn establish_connection<Outbound: OutboundMessage>(
    config: SessionConfig,
    session_ref: InternalSessionRef<Outbound>,
    completion_tx: watch::Sender<bool>,
) {
    loop {
        if session_ref.await_active_session_time().await.is_err() {
            warn!("session task terminated when checking active session time");
            break;
        }

        match connect(&config, session_ref.clone()).await {
            Ok(conn) => {
                if session_ref
                    .register_writer(conn.get_writer())
                    .await
                    .is_err()
                {
                    warn!("session task terminated when trying to register writer");
                    break;
                };
                conn.run_until_disconnect().await;
                warn!("session connection dropped, attempting to reconnect");
            }
            Err(err) => {
                let error_message = err.to_string();
                warn!("failed to connect: {error_message}");
            }
        };

        match session_ref.should_reconnect().await {
            Ok(false) => {
                warn!("session indicated we shouldn't reconnect");
                break;
            }
            Ok(true) => {
                debug!("session indicated we should reconnect");
            }
            Err(_) => {
                warn!("session task terminated when making decision to reconnect");
                break;
            }
        }
        let reconnect_interval = config.reconnect_interval;
        debug!("waiting for {reconnect_interval} seconds before attempting to reconnect");
        sleep(Duration::from_secs(reconnect_interval)).await;
    }

    completion_tx.send_replace(true);
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::application::{Application, InboundDecision, OutboundDecision};
    use crate::message::generate_message;
    use crate::message::logon::{Logon, ResetSeqNumConfig};
    use crate::message::logout::Logout;
    use crate::message::parser::Parser;
    use crate::store::in_memory::InMemoryMessageStore;
    use hotfix_message::Part;
    use hotfix_message::message::Message;
    use hotfix_message::session_fields::MSG_TYPE;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    // Minimal message type for tests
    #[derive(Clone)]
    struct DummyMessage;

    impl OutboundMessage for DummyMessage {
        fn write(&self, _msg: &mut Message) {}
        fn message_type(&self) -> &str {
            "0"
        }
    }

    // No-op application
    struct NoOpApp;

    #[async_trait::async_trait]
    impl Application for NoOpApp {
        type Outbound = DummyMessage;

        async fn on_outbound_message(&self, _msg: &DummyMessage) -> OutboundDecision {
            OutboundDecision::Send
        }
        async fn on_inbound_message(&self, _msg: &Message) -> InboundDecision {
            InboundDecision::Accept
        }
        async fn on_logout(&mut self, _reason: &str) {}
        async fn on_logon(&mut self) {}
    }

    /// A minimal FIX counterparty for testing the Initiator over TCP.
    struct TestCounterparty {
        stream: TcpStream,
        parser: Parser,
        seq_num: u64,
        // Counterparty's view: sender is TEST-TARGET, target is TEST-SENDER
        sender_comp_id: String,
        target_comp_id: String,
    }

    impl TestCounterparty {
        async fn accept(listener: &TcpListener, config: &SessionConfig) -> Self {
            let (stream, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
                .await
                .expect("timeout waiting for connection")
                .expect("failed to accept connection");

            Self {
                stream,
                parser: Parser::default(),
                seq_num: 1,
                // Swap sender/target for counterparty perspective
                sender_comp_id: config.target_comp_id.clone(),
                target_comp_id: config.sender_comp_id.clone(),
            }
        }

        async fn read_message(&mut self) -> Message {
            let mut buf = [0u8; 4096];
            loop {
                let n = self.stream.read(&mut buf).await.expect("read failed");
                if n == 0 {
                    panic!("connection closed before receiving complete message");
                }
                let messages = self.parser.parse(&buf[..n]);
                if let Some(raw_msg) = messages.into_iter().next() {
                    let builder = hotfix_message::MessageBuilder::new(
                        hotfix_message::dict::Dictionary::fix44(),
                        hotfix_message::message::Config::default(),
                    )
                    .expect("failed to create message builder");
                    match builder.build(raw_msg.as_bytes()) {
                        hotfix_message::parsed_message::ParsedMessage::Valid(msg) => return msg,
                        _ => panic!("received invalid FIX message"),
                    }
                }
            }
        }

        async fn expect_message(&mut self, expected_type: &str) -> Message {
            let msg = tokio::time::timeout(Duration::from_secs(2), self.read_message())
                .await
                .expect("timeout waiting for message");
            let msg_type: &str = msg.header().get(MSG_TYPE).expect("missing MSG_TYPE");
            assert_eq!(msg_type, expected_type, "unexpected message type");
            msg
        }

        async fn send_logon(&mut self, heartbeat_interval: u64) {
            let logon = Logon::new(heartbeat_interval, ResetSeqNumConfig::NoReset(None));
            self.send_message(logon).await;
        }

        async fn send_logout(&mut self) {
            self.send_message(Logout::default()).await;
        }

        async fn send_message(&mut self, message: impl OutboundMessage) {
            let raw = generate_message(
                "FIX.4.4",
                &self.sender_comp_id,
                &self.target_comp_id,
                self.seq_num,
                message,
            )
            .expect("failed to generate message");
            self.seq_num += 1;
            self.stream
                .write_all(&raw)
                .await
                .expect("failed to send message");
        }
    }

    fn create_test_config(host: &str, port: u16) -> SessionConfig {
        SessionConfig {
            begin_string: "FIX.4.4".to_string(),
            sender_comp_id: "TEST-SENDER".to_string(),
            target_comp_id: "TEST-TARGET".to_string(),
            data_dictionary_path: None,
            connection_host: host.to_string(),
            connection_port: port,
            tls_config: None,
            heartbeat_interval: 30,
            logon_timeout: 10,
            logout_timeout: 2,
            reconnect_interval: 1, // Short for tests
            reset_on_logon: false,
            schedule: None,
        }
    }

    async fn create_logged_on_initiator() -> (Initiator<DummyMessage>, TestCounterparty) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let config = create_test_config("127.0.0.1", port);

        let initiator = Initiator::start(config.clone(), NoOpApp, InMemoryMessageStore::default())
            .await
            .unwrap();

        let mut counterparty = TestCounterparty::accept(&listener, &config).await;

        // Complete the logon handshake
        counterparty.expect_message("A").await; // Receive Logon
        counterparty.send_logon(30).await; // Send Logon response

        // Give the session a moment to process the logon
        sleep(Duration::from_millis(50)).await;

        (initiator, counterparty)
    }

    #[tokio::test]
    async fn test_start_creates_initiator_successfully() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let config = create_test_config("127.0.0.1", port);

        let initiator = Initiator::start(config, NoOpApp, InMemoryMessageStore::default())
            .await
            .unwrap();

        // Verify initial state
        assert!(!initiator.is_shutdown());
        assert!(initiator.is_interested("TEST-SENDER", "TEST-TARGET"));
        assert!(!initiator.is_interested("WRONG", "TEST-TARGET"));
    }

    #[tokio::test]
    async fn test_initiator_connects_to_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let config = create_test_config("127.0.0.1", port);

        let _initiator = Initiator::start(config, NoOpApp, InMemoryMessageStore::default())
            .await
            .unwrap();

        // Accept the connection from the initiator
        let accept_result = tokio::time::timeout(Duration::from_secs(2), listener.accept()).await;

        assert!(
            accept_result.is_ok(),
            "Initiator should connect to listener"
        );
    }

    #[tokio::test]
    async fn test_initiator_reconnects_after_disconnect() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut config = create_test_config("127.0.0.1", port);
        config.reconnect_interval = 1; // Short interval for test

        let _initiator =
            Initiator::<DummyMessage>::start(config, NoOpApp, InMemoryMessageStore::default())
                .await
                .unwrap();

        // Accept first connection
        let (conn1, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
            .await
            .expect("no connection was established within timeout duration")
            .expect("IO error in connection");

        // Drop the connection to trigger reconnect
        drop(conn1);

        // Should reconnect - accept second connection
        let accept_result = tokio::time::timeout(Duration::from_secs(3), listener.accept()).await;

        assert!(
            accept_result.is_ok(),
            "Initiator should reconnect after disconnect"
        );
    }

    #[tokio::test]
    async fn test_send_delegates_to_session_handle() {
        use crate::session::error::SendOutcome;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let config = create_test_config("127.0.0.1", port);

        let initiator = Initiator::start(config, NoOpApp, InMemoryMessageStore::default())
            .await
            .unwrap();

        // Wait for connection to be established
        let _ = tokio::time::timeout(Duration::from_secs(2), listener.accept())
            .await
            .expect("initiator should connect");

        // Message should be received by session and persisted (seq 2 after Logon)
        let result = initiator.send(DummyMessage).await;
        assert!(matches!(result, Ok(SendOutcome::Sent { .. })));
    }

    #[tokio::test]
    async fn test_send_forget_delegates_to_session_handle() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let config = create_test_config("127.0.0.1", port);

        let initiator = Initiator::start(config, NoOpApp, InMemoryMessageStore::default())
            .await
            .unwrap();

        // Wait for connection to be established
        let _ = tokio::time::timeout(Duration::from_secs(2), listener.accept())
            .await
            .expect("initiator should connect");

        // Message should be successfully queued to the session
        let result = initiator.send_forget(DummyMessage).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_session_handle_returns_working_handle() {
        use crate::session::error::SendOutcome;

        let (initiator, mut counterparty) = create_logged_on_initiator().await;

        // Get the session handle and use it to send a message
        let handle = initiator.session_handle();
        let result = handle.send(DummyMessage).await;

        assert!(matches!(result, Ok(SendOutcome::Sent { .. })));

        // Verify counterparty received the message (msg type "0" = Heartbeat)
        counterparty.expect_message("0").await;
    }

    #[tokio::test]
    async fn test_shutdown_with_logout_handshake() {
        let (initiator, mut counterparty) = create_logged_on_initiator().await;

        assert!(!initiator.is_shutdown());

        // Spawn shutdown in background - it sends Logout and waits for response
        let shutdown_handle = tokio::spawn(async move { initiator.shutdown(false).await });

        // Counterparty receives Logout and responds
        counterparty.expect_message("5").await; // Logout
        counterparty.send_logout().await;

        // Close the TCP connection - this completes the disconnect
        drop(counterparty);

        // Shutdown should complete successfully
        let result = shutdown_handle.await.expect("shutdown task panicked");
        assert!(result.is_ok(), "Shutdown should complete, got {:?}", result);
    }
}
