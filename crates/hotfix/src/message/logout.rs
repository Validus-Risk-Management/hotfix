use crate::message::FixMessage;
use hotfix_message::Part;
use hotfix_message::message::Message;
use hotfix_message::session_fields::TEXT;

#[derive(Clone, Debug, Default)]
pub struct Logout {
    text: Option<String>,
}

impl Logout {
    pub fn with_reason(reason: String) -> Self {
        Self { text: Some(reason) }
    }
}

impl FixMessage for Logout {
    fn write(&self, msg: &mut Message) {
        if let Some(value) = &self.text {
            msg.set(TEXT, value.as_str());
        }
    }

    fn message_type(&self) -> &str {
        "5"
    }

    fn parse(_message: &Message) -> Self {
        unimplemented!()
    }
}
