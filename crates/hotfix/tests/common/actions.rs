use crate::common::mock_counterparty::MockCounterparty;
use crate::common::test_messages::TestMessage;
use hotfix::message::FixMessage;
use hotfix::session::SessionRef;
use std::time::Duration;

pub struct When<T> {
    pub target: T,
}

pub fn when<T>(target: T) -> When<T> {
    When { target }
}

impl When<&SessionRef<TestMessage>> {
    pub async fn requests_disconnect(self) {
        self.target
            .disconnect("Test Session Finished".to_string())
            .await;
    }

    pub async fn sends_message(self, message: TestMessage) {
        self.target.send_message(message).await;
    }
}

impl When<&mut MockCounterparty<TestMessage>> {
    pub async fn has_previously_sent(&mut self, message: impl FixMessage) {
        self.target.push_previously_sent_message(message).await;
    }

    pub async fn resends_message(&mut self, sequence_number: u64) {
        self.target.resend_message(sequence_number).await;
    }

    pub async fn sends_message(&mut self, message: impl FixMessage) {
        self.target.send_message(message).await;
    }

    pub async fn sends_gap_fill(&mut self, start_seq_no: u64, new_seq_no: u64) {
        self.target.send_gap_fill(start_seq_no, new_seq_no).await;
    }

    pub async fn sends_logon(&mut self) {
        self.target.send_logon().await;
    }
}

impl When<Duration> {
    pub async fn elapses(self) {
        tokio::time::advance(self.target).await;
    }
}
