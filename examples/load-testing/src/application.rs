use crate::messages::{ExecutionReport, Message};
use hotfix::Application;
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

pub struct LoadTestingApplication {
    sender: UnboundedSender<ExecutionReport>,
}

impl LoadTestingApplication {
    pub fn new(sender: UnboundedSender<ExecutionReport>) -> Self {
        Self { sender }
    }
}

#[async_trait::async_trait]
impl Application<Message> for LoadTestingApplication {
    async fn on_outbound_message(&self, _msg: &Message) -> anyhow::Result<()> {
        Ok(())
    }

    async fn on_inbound_message(&self, msg: Message) -> anyhow::Result<()> {
        match msg {
            Message::NewOrderSingle(_) => {
                unimplemented!("we should not receive orders");
            }
            Message::Unimplemented(data) => {
                let pretty_bytes: Vec<u8> = data
                    .iter()
                    .map(|b| if *b == b'\x01' { b'|' } else { *b })
                    .collect();
                let s = std::str::from_utf8(&pretty_bytes).unwrap_or("invalid characters");
                info!("received message: {:?}", s);
            }
            Message::ExecutionReport(report) => {
                self.sender.send(report)?;
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
