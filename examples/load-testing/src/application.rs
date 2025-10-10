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
    async fn on_message_from_app(&self, _msg: Message) {
        todo!()
    }

    async fn on_message_to_app(&self, msg: Message) {
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
                self.sender.send(report).unwrap();
            }
        }
    }

    async fn on_logout(&mut self, _reason: &str) {
        info!("we've been logged out");
    }
}
