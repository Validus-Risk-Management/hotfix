use hotfix_message::message::Message;
use std::collections::VecDeque;
use tracing::{debug, error};

use crate::message::parser::RawFixMessage;
use crate::transport::socket_writer::WriterRef;

pub enum SessionState {
    /// We have established a connection, sent a logon message and await a response.
    AwaitingLogon { writer: WriterRef, logon_sent: bool },
    /// We are awaiting the target to resend the gap we have.
    AwaitingResend(AwaitingResendState),
    /// We are in the process of gracefully logging out
    AwaitingLogout { writer: WriterRef }, // we need the writer so we can disconnect it on successful logout
    /// The session is active, we have connected and mutually logged on.
    Active { writer: WriterRef },
    /// The peer has logged us out.
    LoggedOut { reconnect: bool },
    /// The TCP connection has been dropped.
    Disconnected { reconnect: bool, _reason: String },
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
            Self::Active { writer } | Self::AwaitingResend(AwaitingResendState { writer, .. }) => {
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
                match message_type {
                    b"A" => {
                        // Logon message
                        if *logon_sent {
                            error!("trying to send logon twice");
                        } else {
                            writer.send_raw_message(message).await;
                            *logon_sent = true;
                        }
                    }
                    b"5" => {
                        // Logout message
                        writer.send_raw_message(message).await;
                    }
                    _ => error!("invalid outgoing message for AwaitingLogon state"),
                }
            }
            Self::AwaitingLogout { writer } => {
                // Logout messages are allowed because we first transition into AwaitingLogout
                // and only then send the logout message
                if message_type == b"5" {
                    writer.send_raw_message(message).await
                }
            }
            _ => error!("trying to write without an established connection"),
        }
    }

    pub async fn disconnect(&self) {
        match self {
            Self::Active { writer }
            | Self::AwaitingLogon { writer, .. }
            | Self::AwaitingLogout { writer }
            | Self::AwaitingResend(AwaitingResendState { writer, .. }) => writer.disconnect().await,
            _ => debug!("disconnecting an already disconnected session"),
        }
    }

    pub fn try_transition_to_awaiting_logout(&mut self) -> bool {
        match self {
            Self::Active { writer }
            | Self::AwaitingLogon { writer, .. }
            | Self::AwaitingResend(AwaitingResendState { writer, .. }) => {
                *self = SessionState::AwaitingLogout {
                    writer: writer.clone(),
                };
                true
            }
            _ => false,
        }
    }
}

/// Session state we're in while processing messages we requested to be resent.
pub struct AwaitingResendState {
    /// The reference to the writer loop.
    pub(crate) writer: WriterRef,
    /// The end of the gap we're waiting for the target to resend.
    pub(crate) end_seq_number: u64,
    /// Inbound messages we receive while processing the resend.
    pub(crate) inbound_queue: VecDeque<Message>,
}

impl AwaitingResendState {
    pub fn new(writer: WriterRef, end_seq_number: u64) -> Self {
        Self {
            writer,
            end_seq_number,
            inbound_queue: Default::default(),
        }
    }
}
