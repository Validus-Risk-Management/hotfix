use crate::message::OutboundMessage;
use hotfix_message::Part;
use hotfix_message::message::Message;
use hotfix_message::session_fields::TEST_REQ_ID;

#[derive(Clone, Debug)]
pub struct TestRequest {
    test_req_id: String,
}

impl TestRequest {
    pub const MSG_TYPE: &str = "1";

    pub fn new(test_req_id: String) -> Self {
        Self { test_req_id }
    }
}

impl OutboundMessage for TestRequest {
    fn write(&self, msg: &mut Message) {
        msg.set(TEST_REQ_ID, self.test_req_id.as_str());
    }

    fn message_type(&self) -> &str {
        Self::MSG_TYPE
    }
}
