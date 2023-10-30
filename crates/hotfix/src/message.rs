// re-exposing these as applications need this to define their own messages
pub use fefix::definitions::fix44;
pub use fefix::fix_values;
use fefix::tagvalue::{Config, Encoder};
pub use fefix::tagvalue::{EncoderHandle, FvWrite, Message as DecodedMessage};

pub(crate) mod heartbeat;
pub(crate) mod logon;
pub(crate) mod parser;

pub trait FixMessage: Clone + Send + 'static {
    fn write(&self, msg: &mut EncoderHandle<Vec<u8>>);

    fn message_type(&self) -> &[u8];

    fn parse(message: DecodedMessage<&[u8]>) -> Self;
}

pub(crate) fn generate_message(
    sender_comp_id: &str,
    target_comp_id: &str,
    msg_seq_num: usize,
    message: impl FixMessage,
) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut encoder: Encoder<Config> = Encoder::default();
    let mut msg = encoder.start_message(b"FIX.4.4", &mut buffer, message.message_type());
    msg.set_fv(fix44::SENDER_COMP_ID, sender_comp_id);
    msg.set_fv(fix44::TARGET_COMP_ID, target_comp_id.as_bytes());
    msg.set_fv(fix44::MSG_SEQ_NUM, msg_seq_num);
    msg.set_fv(fix44::SENDING_TIME, fix_values::Timestamp::utc_now());

    message.write(&mut msg);

    msg.wrap().to_vec()
}
