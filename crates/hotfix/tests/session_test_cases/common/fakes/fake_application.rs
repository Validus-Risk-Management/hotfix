use crate::common::test_messages::TestMessage;
use hotfix::Application;
use hotfix::application::{InboundDecision, OutboundDecision};
use hotfix_message::message::Message;

pub struct FakeApplication {
    message_sender: tokio::sync::mpsc::UnboundedSender<Message>,
    outbound_decision: OutboundDecision,
}

impl FakeApplication {
    pub fn new(message_sender: tokio::sync::mpsc::UnboundedSender<Message>) -> Self {
        Self {
            message_sender,
            outbound_decision: OutboundDecision::Send,
        }
    }

    pub fn with_outbound_decision(
        message_sender: tokio::sync::mpsc::UnboundedSender<Message>,
        decision: OutboundDecision,
    ) -> Self {
        Self {
            message_sender,
            outbound_decision: decision,
        }
    }
}

#[async_trait::async_trait]
impl Application<TestMessage> for FakeApplication {
    async fn on_outbound_message(&self, _msg: &TestMessage) -> OutboundDecision {
        self.outbound_decision
    }

    async fn on_inbound_message(&self, msg: &Message) -> InboundDecision {
        self.message_sender.send(msg.clone()).unwrap();
        InboundDecision::Accept
    }

    async fn on_logout(&mut self, _reason: &str) {}

    async fn on_logon(&mut self) {}
}
