use crate::common::test_messages::TestMessage;
use hotfix::Application;
use hotfix::application::{InboundDecision, OutboundDecision};

pub struct FakeApplication {
    message_sender: tokio::sync::mpsc::UnboundedSender<TestMessage>,
    outbound_decision: OutboundDecision,
}

impl FakeApplication {
    pub fn new(message_sender: tokio::sync::mpsc::UnboundedSender<TestMessage>) -> Self {
        Self {
            message_sender,
            outbound_decision: OutboundDecision::Send,
        }
    }

    pub fn with_outbound_decision(
        message_sender: tokio::sync::mpsc::UnboundedSender<TestMessage>,
        decision: OutboundDecision,
    ) -> Self {
        Self {
            message_sender,
            outbound_decision: decision,
        }
    }
}

#[async_trait::async_trait]
impl Application<TestMessage, TestMessage> for FakeApplication {
    async fn on_outbound_message(&self, _msg: &TestMessage) -> OutboundDecision {
        self.outbound_decision
    }

    async fn on_inbound_message(&self, msg: TestMessage) -> InboundDecision {
        self.message_sender.send(msg).unwrap();
        InboundDecision::Accept
    }

    async fn on_logout(&mut self, _reason: &str) {}

    async fn on_logon(&mut self) {}
}
