use crate::common::test_messages::TestMessage;
use hotfix::session::{SessionRef, Status};
use std::time::Duration;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);

pub struct Then<T> {
    target: T,
}

pub fn then<T>(target: T) -> Then<T> {
    Then { target }
}

impl Then<&SessionRef<TestMessage>> {
    pub async fn status_changes_to(self, expected_status: Status) {
        self.status_changes_within_time(expected_status, DEFAULT_TIMEOUT)
            .await;
    }

    pub async fn status_changes_within_time(self, expected_status: Status, timeout: Duration) {
        let deadline = tokio::time::Instant::now() + timeout;
        let retry_interval = Duration::from_millis(1);

        let mut session_info = self.target.get_session_info().await;
        while tokio::time::Instant::now() < deadline {
            if session_info.status == expected_status {
                return;
            }
            tokio::time::sleep(retry_interval).await;
            session_info = self.target.get_session_info().await;
        }

        let actual_status = session_info.status;
        panic!(
            "session did not reach expected status within timeout. Expected: {expected_status:?}, Actual: {actual_status:?}"
        );
    }
}
