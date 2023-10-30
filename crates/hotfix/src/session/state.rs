use crate::actors::socket_writer::WriterRef;
use crate::message::parser::RawFixMessage;
use tracing::{debug, error};

pub enum SessionState {
    /// We have established a connection, sent a logon message and await a response.
    AwaitingLogon { writer: WriterRef, logon_sent: bool },
    /// The session is active, we have connected and mutually logged on.
    Active { writer: WriterRef },
    /// The peer has logged us out.
    LoggedOut { reconnect: bool },
    /// The TCP connection has been dropped.
    Disconnected { reconnect: bool, reason: String },
}

impl SessionState {
    pub fn should_reconnect(&self) -> bool {
        match self {
            SessionState::Disconnected { reconnect, .. } => *reconnect,
            _ => true,
        }
    }

    pub async fn send_message(&mut self, message_type: &[u8], message: RawFixMessage) {
        match self {
            Self::Active { writer } => {
                if message_type == b"A" {
                    error!("logon message is invalid for active sessions")
                } else {
                    writer.send_raw_message(message).await
                }
            }
            Self::AwaitingLogon {
                writer,
                ref mut logon_sent,
            } => {
                if message_type == b"A" {
                    if *logon_sent {
                        error!("trying to send logon twice");
                    } else {
                        writer.send_raw_message(message).await;
                        *logon_sent = true;
                    }
                } else {
                    debug!("received message while in logon state - won't send")
                }
            }
            _ => error!("trying to write without an established connection"),
        }
    }

    pub async fn disconnect(&self) {
        match self {
            Self::Active { writer } => writer.disconnect().await,
            _ => debug!("disconnecting an already disconnected session"),
        }
    }
}
