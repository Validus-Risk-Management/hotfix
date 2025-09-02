use crate::common::test_messages::TestMessage;
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

impl When<Duration> {
    pub async fn elapses(self) {
        tokio::time::advance(self.target).await;
    }
}
