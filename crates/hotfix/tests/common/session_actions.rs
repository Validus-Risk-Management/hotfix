use crate::common::test_messages::TestMessage;
use hotfix::session::SessionRef;
use std::time::Duration;

pub trait SessionActions {
    async fn when_disconnect_is_requested(&self);
    async fn when_message_is_sent(&self, message: TestMessage);
}

impl SessionActions for SessionRef<TestMessage> {
    async fn when_disconnect_is_requested(&self) {
        self.disconnect("Test Session Finished".to_string()).await;
    }

    async fn when_message_is_sent(&self, message: TestMessage) {
        self.send_message(message).await;
    }
}

pub async fn when_time_elapses(duration: Duration) {
    tokio::time::advance(duration).await;
}
