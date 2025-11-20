use crate::common::test_messages::TestMessage;
use hotfix::session::SessionRef;

pub struct SessionSpy {
    session: SessionRef<TestMessage>,
    message_receiver: tokio::sync::mpsc::UnboundedReceiver<TestMessage>,
}

impl SessionSpy {
    pub fn new(
        session: SessionRef<TestMessage>,
        message_receiver: tokio::sync::mpsc::UnboundedReceiver<TestMessage>,
    ) -> Self {
        Self {
            session,
            message_receiver,
        }
    }

    pub fn session_ref(&self) -> &SessionRef<TestMessage> {
        &self.session
    }
}
