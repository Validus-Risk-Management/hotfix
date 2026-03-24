use crate::common::test_messages::TestMessage;
use hotfix::Application;
use hotfix::application::{InboundDecision, OutboundDecision};
use hotfix::session::Status;
use hotfix_message::message::Message;
use std::sync::Mutex;

type OutboundDecisionFn = Box<dyn Fn(&TestMessage) -> OutboundDecision + Send>;
type InboundDecisionFn = Box<dyn Fn(&Message) -> InboundDecision + Send>;

pub struct FakeApplication {
    message_sender: tokio::sync::mpsc::UnboundedSender<Message>,
    outbound_decision_fn: Mutex<OutboundDecisionFn>,
    inbound_decision_fn: Mutex<InboundDecisionFn>,
}

impl FakeApplication {
    pub fn builder(
        message_sender: tokio::sync::mpsc::UnboundedSender<Message>,
    ) -> FakeApplicationBuilder {
        FakeApplicationBuilder {
            message_sender,
            outbound_decision_fn: Box::new(|_| OutboundDecision::Send),
            inbound_decision_fn: Box::new(|_| InboundDecision::Accept),
        }
    }
}

pub struct FakeApplicationBuilder {
    message_sender: tokio::sync::mpsc::UnboundedSender<Message>,
    outbound_decision_fn: OutboundDecisionFn,
    inbound_decision_fn: InboundDecisionFn,
}

impl FakeApplicationBuilder {
    pub fn outbound_decision_fn(
        mut self,
        f: impl Fn(&TestMessage) -> OutboundDecision + Send + 'static,
    ) -> Self {
        self.outbound_decision_fn = Box::new(f);
        self
    }

    pub fn inbound_decision_fn(
        mut self,
        f: impl Fn(&Message) -> InboundDecision + Send + 'static,
    ) -> Self {
        self.inbound_decision_fn = Box::new(f);
        self
    }

    pub fn build(self) -> FakeApplication {
        FakeApplication {
            message_sender: self.message_sender,
            outbound_decision_fn: Mutex::new(self.outbound_decision_fn),
            inbound_decision_fn: Mutex::new(self.inbound_decision_fn),
        }
    }
}

#[async_trait::async_trait]
impl Application for FakeApplication {
    type Outbound = TestMessage;

    async fn on_outbound_message(&self, msg: &TestMessage) -> OutboundDecision {
        let decision_fn = self.outbound_decision_fn.lock().unwrap();
        (decision_fn)(msg)
    }

    async fn on_inbound_message(&self, msg: &Message) -> InboundDecision {
        self.message_sender.send(msg.clone()).unwrap();
        let decision_fn = self.inbound_decision_fn.lock().unwrap();
        (decision_fn)(msg)
    }

    async fn on_logout(&mut self, _reason: &str) {}

    async fn on_logon(&mut self) {}

    async fn on_state_change(&self, _from: &Status, _to: &Status) {}
}
