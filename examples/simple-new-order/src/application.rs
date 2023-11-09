use hotfix::Application;
use tracing::info;

use crate::messages::Message;

#[derive(Default)]
pub struct TestApplication {}

#[async_trait::async_trait]
impl Application<Message> for TestApplication {
    async fn on_message_from_app(&self, _msg: Message) {
        todo!()
    }

    async fn on_message_to_app(&self, msg: Message) {
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
    }

    async fn on_logout(&mut self, _reason: &str) {
        info!("we've been logged out");
    }
}
