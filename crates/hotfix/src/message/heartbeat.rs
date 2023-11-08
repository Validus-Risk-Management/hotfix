use hotfix_message::message::Message;

use crate::message::{DecodedMessage, FixMessage};

#[derive(Clone, Debug)]
pub struct Heartbeat;

impl FixMessage for Heartbeat {
    fn write(&self, _msg: &mut Message) {}

    fn message_type(&self) -> &str {
        "0"
    }

    fn parse(_message: DecodedMessage<&[u8]>) -> Self {
        Heartbeat {}
    }
}
