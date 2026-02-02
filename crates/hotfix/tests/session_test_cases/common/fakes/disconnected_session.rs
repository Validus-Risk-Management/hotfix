use crate::session_test_cases::common::test_messages::TestMessage;
use hotfix::session::{InternalSessionRef, SessionHandle};

/// A session with no transport connection established.
///
/// This struct exists to keep the session task alive while testing error paths.
/// When `InternalSessionRef` is converted to `SessionHandle`, only two of three
/// channel senders are cloned - the `event_sender` is not. If the original
/// `InternalSessionRef` is dropped, the `event_sender` closes, causing the
/// session task to terminate before it can process our test messages.
///
/// By holding onto the `InternalSessionRef` (via `_session_ref`), we keep all
/// channels open so the session task can process `send()` calls and return
/// `SendError::Disconnected`.
pub struct DisconnectedSession {
    _session_ref: InternalSessionRef<TestMessage>,
    session_handle: SessionHandle<TestMessage>,
}

impl DisconnectedSession {
    pub fn new(
        session_ref: InternalSessionRef<TestMessage>,
        session_handle: SessionHandle<TestMessage>,
    ) -> Self {
        Self {
            _session_ref: session_ref,
            session_handle,
        }
    }

    pub fn session_handle(&self) -> &SessionHandle<TestMessage> {
        &self.session_handle
    }
}
