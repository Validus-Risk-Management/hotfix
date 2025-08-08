use crate::common::test_messages::TestMessage;
use hotfix::session::SessionRef;

pub trait SessionActions {
    async fn when_disconnect_is_requested(&self);
}

impl SessionActions for SessionRef<TestMessage> {
    async fn when_disconnect_is_requested(&self) {
        self.disconnect("Test Session Finished".to_string()).await;
    }
}
