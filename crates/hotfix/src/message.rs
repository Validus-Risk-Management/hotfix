//! FIX message abstractions to help with encoding and parsing of messages.
use hotfix_message::error::EncodingError as EncodeError;
pub use hotfix_message::field_types::Timestamp;
pub(crate) use hotfix_message::message::{Config, Message};
use hotfix_message::session_fields::{
    MSG_SEQ_NUM, ORIG_SENDING_TIME, POSS_DUP_FLAG, SENDER_COMP_ID, SENDING_TIME, TARGET_COMP_ID,
};
pub use hotfix_message::{Part, RepeatingGroup};

pub mod business_reject;
pub mod heartbeat;
pub mod logon;
pub mod logout;
pub mod parser;
pub mod reject;
pub mod resend_request;
pub mod sequence_reset;
pub mod test_request;
pub mod verification;
pub mod verification_error;

pub use parser::RawFixMessage;
pub use resend_request::ResendRequest;

use heartbeat::Heartbeat;
use logon::Logon;
use logout::Logout;
use reject::Reject;
use sequence_reset::SequenceReset;
use test_request::TestRequest;

static ADMIN_TYPES: [&str; 7] = [
    Logon::MSG_TYPE,
    Heartbeat::MSG_TYPE,
    TestRequest::MSG_TYPE,
    ResendRequest::MSG_TYPE,
    Reject::MSG_TYPE,
    SequenceReset::MSG_TYPE,
    Logout::MSG_TYPE,
];

pub fn is_admin(message_type: &str) -> bool {
    ADMIN_TYPES.contains(&message_type)
}

pub trait OutboundMessage: Clone + Send + 'static {
    fn write(&self, msg: &mut Message);

    fn message_type(&self) -> &str;
}

/// Prepares a FIX message for resend per the FIX spec (PossDupFlag logic).
///
/// Behaviour:
/// - On first resend (no PossDupFlag Y / no OrigSendingTime):
///   * Move current SendingTime(52) to OrigSendingTime(122)
///   * Set SendingTime(52) to the current sending time (may be equal if clock granularity causes no change)
///   * Set PossDupFlag(43)=Y
/// - On subsequent resends (already marked possible duplicate and has OrigSendingTime):
///   * Refresh SendingTime(52) to current time (value may or may not differ from previous)
pub fn prepare_message_for_resend(msg: &mut Message) -> Result<(), &'static str> {
    let header = msg.header_mut();

    if header.get_raw(SENDING_TIME).is_none() {
        return Err("Missing SendingTime (52)");
    }

    let already_poss_dup = header.get::<bool>(POSS_DUP_FLAG).unwrap_or(false);
    let has_orig_sending_time = header.get_raw(ORIG_SENDING_TIME).is_some();

    if already_poss_dup && has_orig_sending_time {
        // Subsequent resend: refresh SendingTime only
        return if header.pop(SENDING_TIME).is_some() {
            header.set(SENDING_TIME, Timestamp::utc_now());
            Ok(())
        } else {
            Err("Failed to extract previous SendingTime")
        };
    }

    // First resend path
    if let Some(original_sending_time_field) = header.pop(SENDING_TIME) {
        let original_ts = Timestamp::parse(&original_sending_time_field.data)
            .ok_or("Invalid original SendingTime format")?;
        header.set(ORIG_SENDING_TIME, original_ts);
        header.set(SENDING_TIME, Timestamp::utc_now());
        header.set(POSS_DUP_FLAG, true);
        Ok(())
    } else {
        Err("Failed to extract original SendingTime")
    }
}

pub fn generate_message(
    begin_string: &str,
    sender_comp_id: &str,
    target_comp_id: &str,
    msg_seq_num: u64,
    message: impl OutboundMessage,
) -> Result<Vec<u8>, EncodeError> {
    let mut msg = Message::new(begin_string, message.message_type());
    msg.set(SENDER_COMP_ID, sender_comp_id);
    msg.set(TARGET_COMP_ID, target_comp_id.as_bytes());
    msg.set(MSG_SEQ_NUM, msg_seq_num);
    msg.set(SENDING_TIME, Timestamp::utc_now());

    message.write(&mut msg);

    msg.encode(&Config::default())
}
