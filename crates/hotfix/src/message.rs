// re-exposing these as applications need this to define their own messages
pub use hotfix_message::field_types::Timestamp;
pub use hotfix_message::fix44;
use hotfix_message::message::{Config, Message};
pub use hotfix_message::Part;

pub(crate) mod heartbeat;
pub(crate) mod logon;
pub(crate) mod parser;

pub trait FixMessage: Clone + Send + 'static {
    fn write(&self, msg: &mut Message);

    fn message_type(&self) -> &str;

    fn parse(message: &Message) -> Self;
}

pub(crate) fn generate_message(
    sender_comp_id: &str,
    target_comp_id: &str,
    msg_seq_num: usize,
    message: impl FixMessage,
) -> Vec<u8> {
    let mut msg = Message::new("FIX.4.4", message.message_type());
    msg.set(fix44::SENDER_COMP_ID, sender_comp_id);
    msg.set(fix44::TARGET_COMP_ID, target_comp_id.as_bytes());
    msg.set(fix44::MSG_SEQ_NUM, msg_seq_num);
    msg.set(fix44::SENDING_TIME, Timestamp::utc_now());

    message.write(&mut msg);

    msg.encode(&Config::default())
}

pub trait WriteMessage {
    fn write(&self, msg: &mut Message);

    fn message_type(&self) -> &str;
}
