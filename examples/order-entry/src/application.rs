use std::sync::{Arc, Mutex};

use crate::messages::OutboundMsg;
use hotfix::Application;
use hotfix::Message;
use hotfix::application::{InboundDecision, OutboundDecision};
use hotfix::message::OutboundMessage;
use hotfix::session::Status;
use hotfix_message::message::Config as EncodeConfig;
use serde::Serialize;
use tracing::info;

#[derive(Clone, Serialize)]
pub struct MessageLogEntry {
    pub id: u64,
    pub direction: &'static str,
    pub fix_string: String,
}

pub struct SharedState {
    pub messages: Vec<MessageLogEntry>,
    next_id: u64,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            next_id: 1,
        }
    }

    pub fn push(&mut self, direction: &'static str, fix_string: String) {
        let id = self.next_id;
        self.next_id += 1;
        self.messages.push(MessageLogEntry {
            id,
            direction,
            fix_string,
        });
    }
}

#[derive(Clone)]
pub struct TestApplication {
    pub shared_state: Arc<Mutex<SharedState>>,
}

impl TestApplication {
    pub fn new(shared_state: Arc<Mutex<SharedState>>) -> Self {
        Self { shared_state }
    }
}

fn encode_pipe_separated(msg: &mut Message) -> String {
    let config = EncodeConfig::with_separator(b'|');
    match msg.encode(&config) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) => format!("<encode error: {e}>"),
    }
}

#[async_trait::async_trait]
impl Application for TestApplication {
    type Outbound = OutboundMsg;

    async fn on_outbound_message(&self, msg: &OutboundMsg) -> OutboundDecision {
        let mut fix_msg = Message::new("FIX.4.4", msg.message_type());
        msg.write(&mut fix_msg);
        let fix_string = encode_pipe_separated(&mut fix_msg);
        if let Ok(mut state) = self.shared_state.lock() {
            state.push("OUT", fix_string);
        }
        OutboundDecision::Send
    }

    async fn on_inbound_message(&self, msg: &Message) -> InboundDecision {
        info!("received inbound message");
        let mut cloned = msg.clone();
        let fix_string = encode_pipe_separated(&mut cloned);
        if let Ok(mut state) = self.shared_state.lock() {
            state.push("IN", fix_string);
        }
        InboundDecision::Accept
    }

    async fn on_logout(&mut self, _reason: &str) {
        info!("we've been logged out");
    }

    async fn on_logon(&mut self) {
        info!("we've been logged in");
    }

    async fn on_state_change(&self, from: &Status, to: &Status) {
        info!("we've changed from {:?} to {:?}", from, to);
    }
}
