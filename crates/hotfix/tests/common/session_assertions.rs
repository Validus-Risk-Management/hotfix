use crate::common::test_messages::TestMessage;
use hotfix::session::{SessionRef, Status};
use std::time::Duration;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(50);

pub trait SessionAssertions {
    async fn then_status_changes_to(&self, expected_status: Status);
    async fn assert_status_with_timeout(&self, expected_status: Status, timeout: Duration);
    async fn when_disconnected(&self);
}

impl SessionAssertions for SessionRef<TestMessage> {
    async fn then_status_changes_to(&self, expected_status: Status) {
        self.assert_status_with_timeout(expected_status, DEFAULT_TIMEOUT)
            .await;
    }

    async fn assert_status_with_timeout(&self, expected_status: Status, timeout: Duration) {
        let deadline = tokio::time::Instant::now() + timeout;
        let retry_interval = Duration::from_millis(5);

        let mut session_info = self.get_session_info().await;
        while tokio::time::Instant::now() < deadline {
            if session_info.status == expected_status {
                return;
            }
            tokio::time::sleep(retry_interval).await;
            session_info = self.get_session_info().await;
        }

        let actual_status = session_info.status;
        panic!(
            "session did not reach expected status within timeout. Expected: {expected_status:?}, Actual: {actual_status:?}"
        );
    }

    async fn when_disconnected(&self) {
        self.disconnect("Test Session Finished".to_string()).await;
    }
}
