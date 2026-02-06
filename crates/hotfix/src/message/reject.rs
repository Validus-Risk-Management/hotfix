use crate::message::OutboundMessage;
use hotfix_message::Part;
use hotfix_message::message::Message;
use hotfix_message::session_fields::{
    MsgType, REF_MSG_TYPE, REF_SEQ_NUM, REF_TAG_ID, SESSION_REJECT_REASON, SessionRejectReason,
    TEXT,
};

#[derive(Clone, Debug)]
pub(crate) struct Reject {
    ref_seq_num: u64,
    ref_tag_id: Option<u64>,
    ref_msg_type: Option<MsgType>,
    session_reject_reason: Option<SessionRejectReason>,
    text: Option<String>,
}

impl Reject {
    pub(crate) fn new(ref_seq_num: u64) -> Self {
        Self {
            ref_seq_num,
            ref_tag_id: None,
            ref_msg_type: None,
            session_reject_reason: None,
            text: None,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn ref_tag_id(mut self, ref_tag_id: u64) -> Self {
        self.ref_tag_id = Some(ref_tag_id);
        self
    }

    #[allow(dead_code)]
    pub(crate) fn ref_msg_type(mut self, ref_msg_type: MsgType) -> Self {
        self.ref_msg_type = Some(ref_msg_type);
        self
    }

    pub(crate) fn session_reject_reason(
        mut self,
        session_reject_reason: SessionRejectReason,
    ) -> Self {
        self.session_reject_reason = Some(session_reject_reason);
        self
    }

    pub(crate) fn text(mut self, text: &str) -> Self {
        self.text = Some(text.to_string());
        self
    }

    #[cfg(test)]
    fn parse(message: &Message) -> Self {
        Self {
            #[allow(clippy::expect_used)]
            ref_seq_num: message
                .get(REF_SEQ_NUM)
                .expect("ref_seq_num should be present"),
            ref_tag_id: message.get(REF_TAG_ID).ok(),
            ref_msg_type: message.get(REF_MSG_TYPE).ok(),
            session_reject_reason: message.get(SESSION_REJECT_REASON).ok(),
            text: message.get::<&str>(TEXT).ok().map(|s| s.to_string()),
        }
    }
}

impl OutboundMessage for Reject {
    fn write(&self, msg: &mut Message) {
        msg.set(REF_SEQ_NUM, self.ref_seq_num);

        if let Some(ref_tag_id) = self.ref_tag_id {
            msg.set(REF_TAG_ID, ref_tag_id);
        }
        if let Some(ref_msg_type) = self.ref_msg_type {
            msg.set(REF_MSG_TYPE, ref_msg_type);
        }
        if let Some(session_reject_reason) = self.session_reject_reason {
            msg.set(SESSION_REJECT_REASON, session_reject_reason);
        }
        if let Some(text) = &self.text {
            msg.set(TEXT, text.as_str());
        }
    }

    fn message_type(&self) -> &str {
        "3"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hotfix_message::message::Message;

    #[test]
    fn test_write_reject_with_required_fields_only() {
        let reject = Reject::new(123);

        let mut msg = Message::new("FIX.4.4", "3");
        reject.write(&mut msg);

        assert_eq!(msg.get::<u64>(REF_SEQ_NUM).unwrap(), 123);
        assert!(msg.get::<u64>(REF_TAG_ID).is_err());
        assert!(msg.get::<MsgType>(REF_MSG_TYPE).is_err());
        assert!(
            msg.get::<SessionRejectReason>(SESSION_REJECT_REASON)
                .is_err()
        );
        assert!(msg.get::<&str>(TEXT).is_err());
    }

    #[test]
    fn test_write_reject_with_all_fields() {
        let reject = Reject::new(456)
            .ref_tag_id(35)
            .ref_msg_type(MsgType::ExecutionReport)
            .session_reject_reason(SessionRejectReason::InvalidTagNumber)
            .text("Invalid message format");

        let mut msg = Message::new("FIX.4.4", "3");
        reject.write(&mut msg);

        assert_eq!(msg.get::<u64>(REF_SEQ_NUM).unwrap(), 456);
        assert_eq!(msg.get::<u64>(REF_TAG_ID).unwrap(), 35);
        assert_eq!(
            msg.get::<MsgType>(REF_MSG_TYPE).unwrap(),
            MsgType::ExecutionReport
        );
        assert_eq!(
            msg.get::<SessionRejectReason>(SESSION_REJECT_REASON)
                .unwrap(),
            SessionRejectReason::InvalidTagNumber
        );
        assert_eq!(msg.get::<&str>(TEXT).unwrap(), "Invalid message format");
    }

    #[test]
    fn test_parse_reject_with_required_fields_only() {
        let mut msg = Message::new("FIX.4.4", "3");
        msg.set(REF_SEQ_NUM, 999u64);

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
        msg.set(REF_SEQ_NUM, 777u64);
        msg.set(REF_TAG_ID, 40u64);
        msg.set(REF_MSG_TYPE, MsgType::OrderSingle);
        msg.set(
            SESSION_REJECT_REASON,
            SessionRejectReason::TagNotDefinedForThisMessageType,
        );
        msg.set(TEXT, "Field not allowed");

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
        let original = Reject::new(555)
            .ref_tag_id(44)
            .ref_msg_type(MsgType::OrderCancelRequest)
            .session_reject_reason(SessionRejectReason::ValueIsIncorrect)
            .text("Price field is invalid");

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
        let original = Reject::new(111);

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
