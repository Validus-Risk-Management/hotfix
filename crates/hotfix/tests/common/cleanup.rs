use crate::common::assertions::{DEFAULT_TIMEOUT, assert_msg_type};
use crate::common::fakes::{FakeCounterparty, SessionSpy};
use crate::common::test_messages::TestMessage;
use hotfix::message::logout::Logout;
use hotfix_message::fix44::MsgType;

pub struct Finally<'a> {
    session: &'a SessionSpy,
    counterparty: &'a mut FakeCounterparty<TestMessage>,
}

pub fn finally<'a>(
    session: &'a SessionSpy,
    counterparty: &'a mut FakeCounterparty<TestMessage>,
) -> Finally<'a> {
    Finally {
        session,
        counterparty,
    }
}

impl<'a> Finally<'a> {
    pub async fn disconnect(self) {
        // initiate disconnect from our side
        self.session.session_handle().shutdown(false).await.unwrap();

        // counterparty receives our logout message
        self.counterparty
            .assert_next_with_timeout(|msg| assert_msg_type(msg, MsgType::Logout), DEFAULT_TIMEOUT)
            .await;

        // counterparty responds with logout acknowledgement
        self.counterparty.send_message(Logout::default()).await;

        // verify disconnection occurs
        self.counterparty
            .assert_disconnected_with_timeout(DEFAULT_TIMEOUT)
            .await;
    }
}
