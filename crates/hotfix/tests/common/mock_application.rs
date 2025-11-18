use crate::common::test_messages::TestMessage;
use hotfix::Application;
use hotfix::application::{InboundDecision, OutboundDecision};

pub struct MockApplication {}

#[async_trait::async_trait]
impl Application<TestMessage> for MockApplication {
    async fn on_outbound_message(&self, _msg: &TestMessage) -> OutboundDecision {
        OutboundDecision::Send
    }

    async fn on_inbound_message(&self, _msg: TestMessage) -> InboundDecision {
        InboundDecision::Accept
    }

    async fn on_logout(&mut self, _reason: &str) {}

    async fn on_logon(&mut self) {}
}
