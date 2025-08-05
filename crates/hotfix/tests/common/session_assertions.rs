use crate::common::test_messages::TestMessage;
use hotfix::session::{SessionRef, Status};
use std::time::Duration;

pub trait SessionAssertions {
    async fn assert_status(&self, expected_status: Status);
    async fn assert_status_with_timeout(&self, expected_status: Status, timeout: Duration);
}

impl SessionAssertions for SessionRef<TestMessage> {
    async fn assert_status(&self, expected_status: Status) {
        self.assert_status_with_timeout(expected_status, Duration::from_millis(10))
            .await;
    }

    async fn assert_status_with_timeout(&self, expected_status: Status, timeout: Duration) {
        let deadline = tokio::time::Instant::now() + timeout;
        let retry_interval = Duration::from_millis(1);

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
}
