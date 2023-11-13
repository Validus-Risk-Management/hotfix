use hotfix_message::message::Message;
use std::collections::VecDeque;
use tracing::{debug, error};

use crate::actors::socket_writer::WriterRef;
use crate::message::parser::RawFixMessage;

pub enum SessionState {
    /// We have established a connection, sent a logon message and await a response.
    AwaitingLogon { writer: WriterRef, logon_sent: bool },
    /// We are awaiting the target to resend the gap we have.
    AwaitingResend(AwaitingResendState),
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
/// Session state we're in while processing messages we requested to be resent.
pub struct AwaitingResendState {
    /// The reference to the writer loop.
    writer: WriterRef,
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
