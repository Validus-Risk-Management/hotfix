use crate::message::FixMessage;
use hotfix_message::fix44::{MsgType, SessionRejectReason};
use hotfix_message::message::Message;
use hotfix_message::{Part, fix44};

#[derive(Clone, Debug)]
struct Reject {
    ref_seq_num: u64,
    ref_tag_id: Option<u64>,
    ref_msg_type: Option<MsgType>,
    session_reject_reason: Option<SessionRejectReason>,
    text: Option<String>,
}

impl FixMessage for Reject {
    fn write(&self, msg: &mut Message) {
        msg.set(fix44::REF_SEQ_NUM, self.ref_seq_num);

        if let Some(ref_tag_id) = self.ref_tag_id {
            msg.set(fix44::REF_TAG_ID, ref_tag_id);
        }
        if let Some(ref_msg_type) = self.ref_msg_type {
            msg.set(fix44::REF_MSG_TYPE, ref_msg_type);
        }
        if let Some(session_reject_reason) = self.session_reject_reason {
            msg.set(fix44::SESSION_REJECT_REASON, session_reject_reason);
        }
        if let Some(text) = &self.text {
            msg.set(fix44::TEXT, text.as_str());
        }
    }

    fn message_type(&self) -> &str {
        "3"
    }

    fn parse(message: &Message) -> Self {
        Self {
            ref_seq_num: message.get(fix44::REF_SEQ_NUM).unwrap(),
            ref_tag_id: message.get(fix44::REF_TAG_ID).ok(),
            ref_msg_type: message.get(fix44::REF_MSG_TYPE).ok(),
            session_reject_reason: message.get(fix44::SESSION_REJECT_REASON).ok(),
            text: message.get::<&str>(fix44::TEXT).ok().map(|s| s.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hotfix_message::fix44::{MsgType, SessionRejectReason};
    use hotfix_message::message::Message;

    #[test]
    fn test_write_reject_with_required_fields_only() {
        let reject = Reject {
            ref_seq_num: 123,
            ref_tag_id: None,
            ref_msg_type: None,
            session_reject_reason: None,
            text: None,
        };

        let mut msg = Message::new("FIX.4.4", "3");
        reject.write(&mut msg);

        assert_eq!(msg.get::<u64>(fix44::REF_SEQ_NUM).unwrap(), 123);
        assert!(msg.get::<u64>(fix44::REF_TAG_ID).is_err());
        assert!(msg.get::<MsgType>(fix44::REF_MSG_TYPE).is_err());
        assert!(
            msg.get::<SessionRejectReason>(fix44::SESSION_REJECT_REASON)
                .is_err()
        );
        assert!(msg.get::<&str>(fix44::TEXT).is_err());
    }

    #[test]
    fn test_write_reject_with_all_fields() {
        let reject = Reject {
            ref_seq_num: 456,
            ref_tag_id: Some(35),
            ref_msg_type: Some(MsgType::ExecutionReport),
            session_reject_reason: Some(SessionRejectReason::InvalidTagNumber),
            text: Some("Invalid message format".to_string()),
        };

        let mut msg = Message::new("FIX.4.4", "3");
        reject.write(&mut msg);

        assert_eq!(msg.get::<u64>(fix44::REF_SEQ_NUM).unwrap(), 456);
        assert_eq!(msg.get::<u64>(fix44::REF_TAG_ID).unwrap(), 35);
        assert_eq!(
            msg.get::<MsgType>(fix44::REF_MSG_TYPE).unwrap(),
            MsgType::ExecutionReport
        );
        assert_eq!(
            msg.get::<SessionRejectReason>(fix44::SESSION_REJECT_REASON)
                .unwrap(),
            SessionRejectReason::InvalidTagNumber
        );
        assert_eq!(
            msg.get::<&str>(fix44::TEXT).unwrap(),
            "Invalid message format"
        );
    }

    #[test]
    fn test_parse_reject_with_required_fields_only() {
        let mut msg = Message::new("FIX.4.4", "3");
        msg.set(fix44::REF_SEQ_NUM, 999u64);

        let parsed = Reject::parse(&msg);

        assert_eq!(parsed.ref_seq_num, 999);
        assert!(parsed.ref_tag_id.is_none());
        assert!(parsed.ref_msg_type.is_none());
        assert!(parsed.session_reject_reason.is_none());
        assert!(parsed.text.is_none());
    }

    #[test]
    fn test_parse_reject_with_all_fields() {
        let mut msg = Message::new("FIX.4.4", "3");
        msg.set(fix44::REF_SEQ_NUM, 777u64);
        msg.set(fix44::REF_TAG_ID, 40u64);
        msg.set(fix44::REF_MSG_TYPE, MsgType::OrderSingle);
        msg.set(
            fix44::SESSION_REJECT_REASON,
            SessionRejectReason::TagNotDefinedForThisMessageType,
        );
        msg.set(fix44::TEXT, "Field not allowed");

        let parsed = Reject::parse(&msg);

        assert_eq!(parsed.ref_seq_num, 777);
        assert_eq!(parsed.ref_tag_id, Some(40));
        assert_eq!(parsed.ref_msg_type, Some(MsgType::OrderSingle));
        assert_eq!(
            parsed.session_reject_reason,
            Some(SessionRejectReason::TagNotDefinedForThisMessageType)
        );
        assert_eq!(parsed.text, Some("Field not allowed".to_string()));
    }

    #[test]
    fn test_round_trip_serialization() {
        let original = Reject {
            ref_seq_num: 555,
            ref_tag_id: Some(44),
            ref_msg_type: Some(MsgType::OrderCancelRequest),
            session_reject_reason: Some(SessionRejectReason::ValueIsIncorrect),
            text: Some("Price field is invalid".to_string()),
        };

        let mut msg = Message::new("FIX.4.4", "3");
        original.write(&mut msg);

        let parsed = Reject::parse(&msg);

        assert_eq!(parsed.ref_seq_num, original.ref_seq_num);
        assert_eq!(parsed.ref_tag_id, original.ref_tag_id);
        assert_eq!(parsed.ref_msg_type, original.ref_msg_type);
        assert_eq!(parsed.session_reject_reason, original.session_reject_reason);
        assert_eq!(parsed.text, original.text);
    }

    #[test]
    fn test_round_trip_with_minimal_fields() {
        let original = Reject {
            ref_seq_num: 111,
            ref_tag_id: None,
            ref_msg_type: None,
            session_reject_reason: None,
            text: None,
        };

        let mut msg = Message::new("FIX.4.4", "3");
        original.write(&mut msg);

        let parsed = Reject::parse(&msg);

        assert_eq!(parsed.ref_seq_num, original.ref_seq_num);
        assert_eq!(parsed.ref_tag_id, original.ref_tag_id);
        assert_eq!(parsed.ref_msg_type, original.ref_msg_type);
        assert_eq!(parsed.session_reject_reason, original.session_reject_reason);
        assert_eq!(parsed.text, original.text);
    }
}
