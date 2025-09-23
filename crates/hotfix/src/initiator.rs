//! FIX initiator implementation.
//!
//! The fIX session that initiates the connection with its peer,
//! the acceptor. Currently, `HotFIX` only supports initiators.
//!
//! The initiator establishes the transport layer connection with
//! the peer, and sends the initial Logon (35=A) message. For transport,
//! `HotFIX` supports plain TCP and encrypted TLS over TCP connections.
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::application::{Application, ApplicationRef};
use crate::config::SessionConfig;
use crate::message::FixMessage;
use crate::session::SessionRef;
use crate::store::MessageStore;
use crate::transport::connect;

pub struct Initiator<M> {
    pub config: SessionConfig,
    session: SessionRef<M>,
    connection_loop_handle: tokio::task::JoinHandle<()>,
}

impl<M: FixMessage> Initiator<M> {
    pub async fn start(
        config: SessionConfig,
        application: impl Application<M>,
        store: impl MessageStore + Send + Sync + 'static,
    ) -> Self {
        let application_ref = ApplicationRef::new(application);
        let session_ref = SessionRef::new(config.clone(), application_ref, store);

        let connection_loop_handle = tokio::spawn({
            let config = config.clone();
            let session_ref = session_ref.clone();
            establish_connection(config, session_ref)
        });

        Self {
            config,
            session: session_ref,
            connection_loop_handle,
        }
    }

    pub async fn send_message(&self, msg: M) {
        self.session.send_message(msg).await;
    }

    pub fn is_interested(&self, sender_comp_id: &str, target_comp_id: &str) -> bool {
        self.config.sender_comp_id == sender_comp_id && self.config.target_comp_id == target_comp_id
    }

    pub fn session_ref(&self) -> SessionRef<M> {
        self.session.clone()
    }

    pub async fn shutdown(self) -> Result<(), tokio::task::JoinError> {
        self.session.shutdown().await;
        self.connection_loop_handle.await
    }
}

async fn establish_connection<M: FixMessage>(config: SessionConfig, session_ref: SessionRef<M>) {
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
}
