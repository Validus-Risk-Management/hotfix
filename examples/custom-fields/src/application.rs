use std::sync::Arc;

use hotfix::Application;
use hotfix::Message;
use hotfix::application::{InboundDecision, OutboundDecision};
use hotfix::message::Part;
use hotfix::session::Status;
use tokio::sync::{Notify, mpsc};
use tracing::{info, warn};

use crate::custom_fix;
use crate::messages::{ExecReportSummary, OutboundMsg};

pub struct TestApplication {
    pub logon_signal: Arc<Notify>,
    pub exec_tx: mpsc::UnboundedSender<ExecReportSummary>,
}

#[async_trait::async_trait]
impl Application for TestApplication {
    type Outbound = OutboundMsg;

    async fn on_outbound_message(&self, _msg: &OutboundMsg) -> OutboundDecision {
        OutboundDecision::Send
    }

    async fn on_inbound_message(&self, msg: &Message) -> InboundDecision {
        let msg_type: Result<&str, _> = msg.header().get(custom_fix::MSG_TYPE);
        if !matches!(msg_type, Ok("8")) {
            return InboundDecision::Accept;
        }

        let cl_ord_id: Result<&str, _> = msg.get(custom_fix::CL_ORD_ID);
        let ord_status: Result<custom_fix::OrdStatus, _> = msg.get(custom_fix::ORD_STATUS);
        let client_strategy_id: Option<i32> = msg.get(custom_fix::CLIENT_STRATEGY_ID).ok();

        match (cl_ord_id, ord_status) {
            (Ok(cl_ord_id), Ok(ord_status)) => {
                let summary = ExecReportSummary {
                    cl_ord_id: cl_ord_id.to_string(),
                    ord_status,
                    client_strategy_id,
                };
                if let Err(err) = self.exec_tx.send(summary) {
                    warn!("failed to forward execution report: {err}");
                }
            }
            _ => warn!("execution report missing ClOrdID or OrdStatus"),
        }

        InboundDecision::Accept
    }

    async fn on_logout(&mut self, reason: &str) {
        info!("logged out: {reason}");
    }

    async fn on_logon(&mut self) {
        info!("logged on");
        self.logon_signal.notify_one();
    }

    async fn on_state_change(&self, from: &Status, to: &Status) {
        info!("session state changed: {from:?} -> {to:?}");
    }
}
