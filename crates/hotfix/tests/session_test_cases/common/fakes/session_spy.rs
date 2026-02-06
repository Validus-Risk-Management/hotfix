use crate::common::test_messages::TestMessage;
use hotfix::session::SessionHandle;
use hotfix_message::message::Message;

pub struct SessionSpy {
    session_handle: SessionHandle<TestMessage>,
    message_receiver: tokio::sync::mpsc::UnboundedReceiver<Message>,
}

impl SessionSpy {
    pub fn new(
        session_handle: SessionHandle<TestMessage>,
        message_receiver: tokio::sync::mpsc::UnboundedReceiver<Message>,
    ) -> Self {
        Self {
            session_handle,
            message_receiver,
        }
    }

    pub fn session_handle(&self) -> &SessionHandle<TestMessage> {
        &self.session_handle
    }

    pub async fn assert_next_with_timeout<F>(&mut self, assertion: F, timeout: std::time::Duration)
    where
        F: FnOnce(&Message),
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
