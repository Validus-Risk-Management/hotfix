use hotfix::Application;
use tracing::info;

use crate::messages::Message;

#[derive(Default)]
pub struct TestApplication {}

#[async_trait::async_trait]
impl Application<Message> for TestApplication {
    async fn on_outbound_message(&self, _msg: &Message) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_inbound_message(&self, msg: Message) -> anyhow::Result<()> {
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
        Ok(())
    }

    async fn on_logout(&mut self, _reason: &str) -> anyhow::Result<()> {
        info!("we've been logged out");
        Ok(())
    }

    async fn on_logon(&mut self) -> anyhow::Result<()> {
        info!("we've been logged in");
        Ok(())
    }
}
