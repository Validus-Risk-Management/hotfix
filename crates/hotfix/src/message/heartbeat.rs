use crate::message::{InboundMessage, OutboundMessage};
use hotfix_message::Part;
use hotfix_message::message::Message;
use hotfix_message::session_fields::TEST_REQ_ID;

#[derive(Clone, Debug, Default)]
pub struct Heartbeat {
    test_req_id: Option<String>,
}

impl Heartbeat {
    pub fn for_request(test_req_id: String) -> Self {
        Self {
            test_req_id: Some(test_req_id),
        }
    }
}

impl OutboundMessage for Heartbeat {
    fn write(&self, msg: &mut Message) {
        if let Some(req_id) = &self.test_req_id {
            msg.set(TEST_REQ_ID, req_id.as_str());
        }
    }

    fn message_type(&self) -> &str {
        "0"
    }
}

impl InboundMessage for Heartbeat {
    fn parse(_message: &Message) -> Self {
        // TODO: this needs to be implemented properly when we're implementing Test Requests
        Heartbeat { test_req_id: None }
    }
}
