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

    pub async fn assert_next_with_timeout<F>(&mut self, assertion: F, timeout: std::time::Duration)
    where
        F: FnOnce(&TestMessage),
    {
        match tokio::time::timeout(timeout, self.message_receiver.recv()).await {
            Ok(Some(message)) => {
                assertion(&message);
            }
            Ok(None) => {
                panic!("disconnected before receiving any message");
            }
            Err(_) => {
                panic!("timeout expired before receiving any message");
            }
        }
    }
}
