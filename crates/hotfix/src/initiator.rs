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
use crate::message::FixMessage;
use crate::session::{InternalSessionRef, SessionHandle};
use crate::store::MessageStore;
use crate::transport::connect;

#[derive(Clone)]
pub struct Initiator<M> {
    pub config: SessionConfig,
    session_handle: SessionHandle<M>,
    completion_rx: watch::Receiver<bool>,
}

impl<M: FixMessage> Initiator<M> {
    pub async fn start(
        config: SessionConfig,
        application: impl Application<M>,
        store: impl MessageStore + Send + Sync + 'static,
    ) -> Self {
        let session_ref = InternalSessionRef::new(config.clone(), application, store);
        let (completion_tx, completion_rx) = watch::channel(false);

        tokio::spawn({
            let config = config.clone();
            let session_ref = session_ref.clone();
            establish_connection(config, session_ref, completion_tx)
        });

        Self {
            config,
            session_handle: session_ref.into(),
            completion_rx,
        }
    }

    pub async fn send_message(&self, msg: M) -> anyhow::Result<()> {
        self.session_handle.send_message(msg).await?;

        Ok(())
    }

    pub fn is_interested(&self, sender_comp_id: &str, target_comp_id: &str) -> bool {
        self.config.sender_comp_id == sender_comp_id && self.config.target_comp_id == target_comp_id
    }

    pub fn session_handle(&self) -> SessionHandle<M> {
        self.session_handle.clone()
    }

    pub async fn shutdown(self, reconnect: bool) -> anyhow::Result<()> {
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

async fn establish_connection<M: FixMessage>(
    config: SessionConfig,
    session_ref: InternalSessionRef<M>,
    completion_tx: watch::Sender<bool>,
) {
    loop {
        session_ref.await_active_session_time().await;

        match connect(&config, session_ref.clone()).await {
            Ok(conn) => {
                session_ref.register_writer(conn.get_writer()).await;
                conn.run_until_disconnect().await;
                warn!("session connection dropped, attempting to reconnect");
            }
            Err(err) => {
                let error_message = err.to_string();
                warn!("failed to connect: {error_message}");
            }
        };

        if !session_ref.should_reconnect().await {
            warn!("session indicated we shouldn't reconnect");
            break;
        }
        let reconnect_interval = config.reconnect_interval;
        debug!("waiting for {reconnect_interval} seconds before attempting to reconnect");
        sleep(Duration::from_secs(reconnect_interval)).await;
    }

    completion_tx.send_replace(true);
}
