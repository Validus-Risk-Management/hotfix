use crate::application::BusinessRejectReason;
use crate::message::OutboundMessage;
use hotfix_message::dict::{FieldLocation, FixDatatype};
use hotfix_message::message::Message;
use hotfix_message::session_fields::{REF_MSG_TYPE, REF_SEQ_NUM, TEXT};
use hotfix_message::{Buffer, FieldType, HardCodedFixFieldDefinition, Part};

const BUSINESS_REJECT_REASON: &HardCodedFixFieldDefinition = &HardCodedFixFieldDefinition {
    name: "BusinessRejectReason",
    tag: 380,
    data_type: FixDatatype::Int,
    location: FieldLocation::Body,
};

impl<'a> FieldType<'a> for BusinessRejectReason {
    type Error = ();
    type SerializeSettings = ();

    fn serialize_with<B>(&self, buffer: &mut B, _settings: Self::SerializeSettings) -> usize
    where
        B: Buffer,
    {
        let value = *self as u32;
        value.serialize(buffer)
    }

    fn deserialize(data: &'a [u8]) -> Result<Self, Self::Error> {
        let value = u32::deserialize(data).map_err(|_| ())?;
        match value {
            0 => Ok(Self::Other),
            1 => Ok(Self::UnknownId),
            2 => Ok(Self::UnknownSecurity),
            3 => Ok(Self::UnsupportedMessageType),
            4 => Ok(Self::ApplicationNotAvailable),
            5 => Ok(Self::ConditionallyRequiredFieldMissing),
            6 => Ok(Self::NotAuthorized),
            7 => Ok(Self::DeliverToFirmNotAvailable),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct BusinessReject {
    ref_msg_type: String,
    reason: BusinessRejectReason,
    ref_seq_num: Option<u64>,
    text: Option<String>,
}

impl BusinessReject {
    pub(crate) const MSG_TYPE: &str = "j";

    pub(crate) fn new(ref_msg_type: &str, reason: BusinessRejectReason) -> Self {
        Self {
            ref_msg_type: ref_msg_type.to_string(),
            reason,
            ref_seq_num: None,
            text: None,
        }
    }

    pub(crate) fn ref_seq_num(mut self, ref_seq_num: u64) -> Self {
        self.ref_seq_num = Some(ref_seq_num);
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
            ref_msg_type: message
                .get::<&str>(REF_MSG_TYPE)
                .expect("ref_msg_type should be present")
                .to_string(),
            #[allow(clippy::expect_used)]
            reason: message
                .get(BUSINESS_REJECT_REASON)
                .expect("reason should be present"),
            ref_seq_num: message.get(REF_SEQ_NUM).ok(),
            text: message.get::<&str>(TEXT).ok().map(|s| s.to_string()),
        }
    }
}

impl OutboundMessage for BusinessReject {
    fn write(&self, msg: &mut Message) {
        msg.set(REF_MSG_TYPE, self.ref_msg_type.as_str());
        msg.set(BUSINESS_REJECT_REASON, self.reason);

        if let Some(ref_seq_num) = self.ref_seq_num {
            msg.set(REF_SEQ_NUM, ref_seq_num);
        }
        if let Some(text) = &self.text {
            msg.set(TEXT, text.as_str());
        }
    }

    fn message_type(&self) -> &str {
        Self::MSG_TYPE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hotfix_message::message::Message;

    #[test]
    fn test_write_business_reject_with_required_fields_only() {
        let reject = BusinessReject::new("D", BusinessRejectReason::UnsupportedMessageType);

        let mut msg = Message::new("FIX.4.4", "j");
        reject.write(&mut msg);

        assert_eq!(msg.get::<&str>(REF_MSG_TYPE).unwrap(), "D");
        assert_eq!(
            msg.get::<BusinessRejectReason>(BUSINESS_REJECT_REASON)
                .unwrap(),
            BusinessRejectReason::UnsupportedMessageType
        );
        assert!(msg.get::<u64>(REF_SEQ_NUM).is_err());
        assert!(msg.get::<&str>(TEXT).is_err());
    }

    #[test]
    fn test_write_business_reject_with_all_fields() {
        let reject = BusinessReject::new("8", BusinessRejectReason::NotAuthorized)
            .ref_seq_num(456)
            .text("Not authorized for execution reports");

        let mut msg = Message::new("FIX.4.4", "j");
        reject.write(&mut msg);

        assert_eq!(msg.get::<&str>(REF_MSG_TYPE).unwrap(), "8");
        assert_eq!(
            msg.get::<BusinessRejectReason>(BUSINESS_REJECT_REASON)
                .unwrap(),
            BusinessRejectReason::NotAuthorized
        );
        assert_eq!(msg.get::<u64>(REF_SEQ_NUM).unwrap(), 456);
        assert_eq!(
            msg.get::<&str>(TEXT).unwrap(),
            "Not authorized for execution reports"
        );
    }

    #[test]
    fn test_round_trip_serialization() {
        let original =
            BusinessReject::new("D", BusinessRejectReason::ConditionallyRequiredFieldMissing)
                .ref_seq_num(789)
                .text("ClOrdID is required");

        let mut msg = Message::new("FIX.4.4", "j");
        original.write(&mut msg);

        let parsed = BusinessReject::parse(&msg);

        assert_eq!(parsed.ref_msg_type, original.ref_msg_type);
        assert_eq!(parsed.reason, original.reason);
        assert_eq!(parsed.ref_seq_num, original.ref_seq_num);
        assert_eq!(parsed.text, original.text);
    }

    #[test]
    fn test_round_trip_with_minimal_fields() {
        let original = BusinessReject::new("0", BusinessRejectReason::Other);

        let mut msg = Message::new("FIX.4.4", "j");
        original.write(&mut msg);

        let parsed = BusinessReject::parse(&msg);

        assert_eq!(parsed.ref_msg_type, original.ref_msg_type);
        assert_eq!(parsed.reason, original.reason);
        assert_eq!(parsed.ref_seq_num, original.ref_seq_num);
        assert_eq!(parsed.text, original.text);
    }

    #[test]
    fn test_message_type() {
        let reject = BusinessReject::new("D", BusinessRejectReason::Other);
        assert_eq!(reject.message_type(), "j");
    }

    #[test]
    fn test_all_reject_reasons_round_trip() {
        let reasons = [
            BusinessRejectReason::Other,
            BusinessRejectReason::UnknownId,
            BusinessRejectReason::UnknownSecurity,
            BusinessRejectReason::UnsupportedMessageType,
            BusinessRejectReason::ApplicationNotAvailable,
            BusinessRejectReason::ConditionallyRequiredFieldMissing,
            BusinessRejectReason::NotAuthorized,
            BusinessRejectReason::DeliverToFirmNotAvailable,
        ];

        for reason in reasons {
            let reject = BusinessReject::new("D", reason);
            let mut msg = Message::new("FIX.4.4", "j");
            reject.write(&mut msg);

            let parsed = BusinessReject::parse(&msg);
            assert_eq!(parsed.reason, reason, "Round-trip failed for {reason:?}");
        }
    }
}
