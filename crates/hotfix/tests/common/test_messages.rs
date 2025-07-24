use hotfix::Message as HotfixMessage;
use hotfix::message::FixMessage;

#[derive(Debug, Clone)]
pub struct TestMessage;

impl FixMessage for TestMessage {
    fn write(&self, _msg: &mut HotfixMessage) {}

    fn message_type(&self) -> &str {
        unimplemented!()
    }

    fn parse(_msg: &HotfixMessage) -> Self {
        TestMessage
    }
}
