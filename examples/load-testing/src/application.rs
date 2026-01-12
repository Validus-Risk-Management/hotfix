use crate::messages::{ExecutionReport, InboundMsg, OutboundMsg};
use hotfix::Application;
use hotfix::application::{InboundDecision, OutboundDecision};
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
impl Application<InboundMsg, OutboundMsg> for LoadTestingApplication {
    async fn on_outbound_message(&self, _msg: &OutboundMsg) -> OutboundDecision {
        OutboundDecision::Send
    }

    async fn on_inbound_message(&self, msg: InboundMsg) -> InboundDecision {
        match msg {
            InboundMsg::Unimplemented(data) => {
                let pretty_bytes: Vec<u8> = data
                    .iter()
                    .map(|b| if *b == b'\x01' { b'|' } else { *b })
                    .collect();
                let s = std::str::from_utf8(&pretty_bytes).unwrap_or("invalid characters");
                info!("received message: {:?}", s);
            }
            InboundMsg::ExecutionReport(report) => {
                if self.sender.send(report).is_err() {
                    return InboundDecision::TerminateSession;
                }
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
