use crate::common::fakes::{FakeCounterparty, SessionSpy};
use crate::common::test_messages::TestMessage;
use hotfix::session::SessionHandle;
use hotfix::session::Status;
use hotfix_message::fix44::{MSG_TYPE, MsgType};
use hotfix_message::message::Message;
use hotfix_message::{FieldType, Part};
use std::time::Duration;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);

pub struct Then<T> {
    target: T,
}

pub fn then<T>(target: T) -> Then<T> {
    Then { target }
}

impl Then<&mut SessionSpy> {
    fn session_handle(&self) -> &SessionHandle<TestMessage> {
        self.target.session_handle()
    }

    pub async fn receives<F>(self, assertion: F)
    where
        F: FnOnce(&Message),
    {
        self.target
            .assert_next_with_timeout(assertion, DEFAULT_TIMEOUT)
            .await;
    }

    pub async fn target_sequence_number_reaches(self, expected_target_sequence_number: u64) {
        let timeout = DEFAULT_TIMEOUT;
        let deadline = tokio::time::Instant::now() + timeout;
        let retry_interval = Duration::from_millis(1);

        let mut session_info = self.session_handle().get_session_info().await.unwrap();
        while tokio::time::Instant::now() < deadline {
            if session_info.next_target_seq_number - 1 == expected_target_sequence_number {
                return;
            }
            tokio::time::sleep(retry_interval).await;
            session_info = self.session_handle().get_session_info().await.unwrap();
        }

        let actual_target_seq_number = session_info.next_target_seq_number - 1;
        panic!(
            "session did not reach target sequence number within timeout. Expected: {expected_target_sequence_number}, Actual: {actual_target_seq_number}"
        );
    }

    pub async fn status_changes_to(self, expected_status: Status) {
        self.status_changes_within_time(expected_status, DEFAULT_TIMEOUT)
            .await;
    }

    pub async fn status_changes_within_time(self, expected_status: Status, timeout: Duration) {
        let deadline = tokio::time::Instant::now() + timeout;
        let retry_interval = Duration::from_millis(1);

        let mut session_info = self.session_handle().get_session_info().await.unwrap();
        while tokio::time::Instant::now() < deadline {
            if session_info.status == expected_status {
                return;
            }
            tokio::time::sleep(retry_interval).await;
            session_info = self.session_handle().get_session_info().await.unwrap();
        }

        let actual_status = session_info.status;
        panic!(
            "session did not reach expected status within timeout. Expected: {expected_status:?}, Actual: {actual_status:?}"
        );
    }
}

impl Then<&mut FakeCounterparty<TestMessage>> {
    pub async fn receives<F>(self, assertion: F)
    where
        F: FnOnce(&Message),
    {
        self.target
            .assert_next_with_timeout(assertion, DEFAULT_TIMEOUT)
            .await;
    }

    pub async fn gets_disconnected(self) {
        self.target
            .assert_disconnected_with_timeout(DEFAULT_TIMEOUT)
            .await;
    }
}

pub fn assert_msg_type(msg: &Message, msg_type: MsgType) {
    assert_eq!(
        msg.header().get::<&str>(MSG_TYPE).unwrap(),
        msg_type.to_string()
    )
}
