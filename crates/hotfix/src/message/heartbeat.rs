use fefix::tagvalue::EncoderHandle;

use crate::message::{DecodedMessage, FixMessage};

#[derive(Clone, Debug)]
pub struct Heartbeat;

impl FixMessage for Heartbeat {
    fn write(&self, _msg: &mut EncoderHandle<Vec<u8>>) {}

    fn message_type(&self) -> &[u8] {
        b"0"
    }

    fn parse(_message: DecodedMessage<&[u8]>) -> Self {
        Heartbeat {}
    }
}
