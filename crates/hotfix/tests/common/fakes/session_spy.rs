use crate::common::test_messages::TestMessage;
use hotfix::session::{InternalSessionRef, SessionHandle};

pub struct SessionSpy {
    session: InternalSessionRef<TestMessage>,
    session_handle: SessionHandle<TestMessage>,
    message_receiver: tokio::sync::mpsc::UnboundedReceiver<TestMessage>,
}

impl SessionSpy {
    pub fn new(
        session: InternalSessionRef<TestMessage>,
        message_receiver: tokio::sync::mpsc::UnboundedReceiver<TestMessage>,
    ) -> Self {
        Self {
            session: session.clone(),
            session_handle: session.into(),
            message_receiver,
        }
    }

    pub fn session_ref(&self) -> &InternalSessionRef<TestMessage> {
        &self.session
    }

    pub fn session_handle(&self) -> &SessionHandle<TestMessage> {
        &self.session_handle
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
