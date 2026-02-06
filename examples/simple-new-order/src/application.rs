use crate::messages::OutboundMsg;
use hotfix::Application;
use hotfix::Message;
use hotfix::application::{InboundDecision, OutboundDecision};
use tracing::info;

#[derive(Default)]
pub struct TestApplication {}

#[async_trait::async_trait]
impl Application for TestApplication {
    type Outbound = OutboundMsg;

    async fn on_outbound_message(&self, _msg: &OutboundMsg) -> OutboundDecision {
        OutboundDecision::Send
    }

    async fn on_inbound_message(&self, _msg: &Message) -> InboundDecision {
        info!("received inbound message");
        InboundDecision::Accept
    }

    async fn on_logout(&mut self, _reason: &str) {
        info!("we've been logged out");
    }

    async fn on_logon(&mut self) {
        info!("we've been logged in");
    }
}
