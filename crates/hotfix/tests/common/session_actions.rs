use crate::common::test_messages::TestMessage;
use hotfix::session::SessionRef;
use std::time::Duration;

pub struct When<'a, T> {
    pub target: &'a mut T,
}

pub fn when<'a, T>(target: &'a mut T) -> When<'a, T> {
    When { target }
}

impl<'a> When<'a, SessionRef<TestMessage>> {
    pub async fn requests_disconnect(&self) {
        self.target
            .disconnect("Test Session Finished".to_string())
            .await;
    }

    pub async fn sends_message(&self, message: TestMessage) {
        self.target.send_message(message).await;
    }
}

pub async fn when_time_elapses(duration: Duration) {
    tokio::time::advance(duration).await;
}
