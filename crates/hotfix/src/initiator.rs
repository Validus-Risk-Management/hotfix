use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::actors::application::{Application, ApplicationRef};
use crate::config::SessionConfig;
use crate::message::FixMessage;
use crate::session::SessionRef;
use crate::store::MessageStore;
use crate::transport::FixConnection;

pub struct Initiator<M> {
    pub config: SessionConfig,
    session: SessionRef<M>,
}

impl<M: FixMessage> Initiator<M> {
    pub async fn new(
        config: SessionConfig,
        application: impl Application<M>,
        store: impl MessageStore + Send + Sync + 'static,
    ) -> Self {
        let application_ref = ApplicationRef::new(application);
        let session_ref = SessionRef::new(config.clone(), application_ref, store);

        tokio::spawn({
            let config = config.clone();
            let session_ref = session_ref.clone();
            establish_connection(config, session_ref)
        });

        Self {
            config,
            session: session_ref,
        }
    }

    pub async fn send_message(&self, msg: M) {
        self.session.send_message(msg).await;
    }

    pub fn is_interested(&self, sender_comp_id: &str, target_comp_id: &str) -> bool {
        self.config.sender_comp_id == sender_comp_id && self.config.target_comp_id == target_comp_id
    }
}

async fn establish_connection<M: FixMessage>(config: SessionConfig, session_ref: SessionRef<M>) {
    loop {
        if !session_ref.should_reconnect().await {
            warn!("session indicated we shouldn't reconnect");
            break;
        }

        match FixConnection::connect(&config, session_ref.clone()).await {
            Ok(conn) => {
                session_ref.register_writer(conn.get_writer()).await;
                conn.run_until_disconnect().await;

                warn!("session connection dropped, attempting to reconnect");
            }
            Err(err) => {
                let error_message = err.to_string();
                warn!("failed to connect: {error_message}");

                let reconnect_interval = config.reconnect_interval;
                debug!("waiting for {reconnect_interval} seconds before attempting to reconnect");
                sleep(Duration::from_secs(reconnect_interval)).await;
            }
        };
    }
}
