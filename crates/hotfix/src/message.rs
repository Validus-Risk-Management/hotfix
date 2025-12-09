//! FIX message abstractions to help with encoding and parsing of messages.
use hotfix_message::error::EncodingError as EncodeError;
pub use hotfix_message::field_types::Timestamp;
pub(crate) use hotfix_message::message::{Config, Message};
use hotfix_message::session_fields::{MSG_SEQ_NUM, SENDER_COMP_ID, SENDING_TIME, TARGET_COMP_ID};
pub use hotfix_message::{Part, RepeatingGroup};

pub mod heartbeat;
pub mod logon;
pub mod logout;
pub mod parser;
pub mod reject;
pub mod resend_request;
pub mod sequence_reset;
pub mod test_request;
pub mod verification;

pub use parser::RawFixMessage;
pub use resend_request::ResendRequest;

pub trait FixMessage: Clone + Send + 'static {
    fn write(&self, msg: &mut Message);

    fn message_type(&self) -> &str;

    fn parse(message: &Message) -> Self;
}

pub fn generate_message(
    begin_string: &str,
    sender_comp_id: &str,
    target_comp_id: &str,
    msg_seq_num: u64,
    message: impl FixMessage,
) -> Result<Vec<u8>, EncodeError> {
    let mut msg = Message::new(begin_string, message.message_type());
    msg.set(SENDER_COMP_ID, sender_comp_id);
    msg.set(TARGET_COMP_ID, target_comp_id.as_bytes());
    msg.set(MSG_SEQ_NUM, msg_seq_num);
    msg.set(SENDING_TIME, Timestamp::utc_now());

    message.write(&mut msg);

    msg.encode(&Config::default())
}

pub trait WriteMessage {
    fn write(&self, msg: &mut Message);

    fn message_type(&self) -> &str;
}
