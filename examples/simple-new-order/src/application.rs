use crate::messages::Message;
use hotfix::Application;
use hotfix::application::{InboundDecision, OutboundDecision};
use tracing::info;

#[derive(Default)]
pub struct TestApplication {}

#[async_trait::async_trait]
impl Application<Message> for TestApplication {
    async fn on_outbound_message(&self, _msg: &Message) -> OutboundDecision {
        OutboundDecision::Send
    }

    async fn on_inbound_message(&self, msg: Message) -> InboundDecision {
        match msg {
            Message::NewOrderSingle(_) => {
                unimplemented!("we should not receive orders");
            }
            Message::UnimplementedMessage(data) => {
                let pretty_bytes: Vec<u8> = data
                    .iter()
                    .map(|b| if *b == b'\x01' { b'|' } else { *b })
                    .collect();
                let s = std::str::from_utf8(&pretty_bytes).unwrap_or("invalid characters");
                info!("received message: {:?}", s);
            }
        }

        InboundDecision::Accept
    }

    async fn on_logout(&mut self, _reason: &str) {
        info!("we've been logged out");
    }

    async fn on_logon(&mut self) {
        info!("we've been logged in");
    }
}
