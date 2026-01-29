//! FIX initiator implementation.
//!
//! The fIX session that initiates the connection with its peer,
//! the acceptor. Currently, `HotFIX` only supports initiators.
//!
//! The initiator establishes the transport layer connection with
//! the peer, and sends the initial Logon (35=A) message. For transport,
//! `HotFIX` supports plain TCP and encrypted TLS over TCP connections.
use anyhow::Result;
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::application::Application;
use crate::config::SessionConfig;
use crate::message::{InboundMessage, OutboundMessage};
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
    pub async fn start<Inbound: InboundMessage>(
        config: SessionConfig,
        application: impl Application<Inbound, Outbound>,
        store: impl MessageStore + 'static,
    ) -> Result<Self> {
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

    pub async fn send_message(&self, msg: Outbound) -> Result<()> {
        self.session_handle.send_message(msg).await?;

        Ok(())
    }

    pub fn is_interested(&self, sender_comp_id: &str, target_comp_id: &str) -> bool {
        self.config.sender_comp_id == sender_comp_id && self.config.target_comp_id == target_comp_id
    }

    pub fn session_handle(&self) -> SessionHandle<Outbound> {
        self.session_handle.clone()
    }

    pub async fn shutdown(self, reconnect: bool) -> Result<()> {
        self.session_handle.shutdown(reconnect).await?;
        tokio::time::timeout(Duration::from_secs(5), self.wait_for_shutdown()).await?;

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

#[cfg(all(test, feature = "fix44"))]
mod tests {
    use super::*;
    use crate::application::{Application, InboundDecision, OutboundDecision};
    use crate::message::InboundMessage;
    use crate::store::in_memory::InMemoryMessageStore;
    use hotfix_message::message::Message;
    use std::time::Duration;
    use tokio::net::TcpListener;

    // Minimal message type for tests
    #[derive(Clone)]
    struct DummyMessage;

    impl OutboundMessage for DummyMessage {
        fn write(&self, _msg: &mut Message) {}
        fn message_type(&self) -> &str {
            "0"
        }
    }

    impl InboundMessage for DummyMessage {
        fn parse(_message: &Message) -> Self {
            DummyMessage
        }
    }

    // No-op application
    struct NoOpApp;

    #[async_trait::async_trait]
    impl Application<DummyMessage, DummyMessage> for NoOpApp {
        async fn on_outbound_message(&self, _msg: &DummyMessage) -> OutboundDecision {
            OutboundDecision::Send
        }
        async fn on_inbound_message(&self, _msg: DummyMessage) -> InboundDecision {
            InboundDecision::Accept
        }
        async fn on_logout(&mut self, _reason: &str) {}
        async fn on_logon(&mut self) {}
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

        let _initiator = Initiator::<DummyMessage>::start::<DummyMessage>(
            config,
            NoOpApp,
            InMemoryMessageStore::default(),
        )
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
}
