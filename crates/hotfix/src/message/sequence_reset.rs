use crate::message::OutboundMessage;
use hotfix_message::Part;
use hotfix_message::field_types::Timestamp;
use hotfix_message::message::Message;
use hotfix_message::session_fields::{
    GAP_FILL_FLAG, NEW_SEQ_NO, ORIG_SENDING_TIME, POSS_DUP_FLAG, SENDING_TIME,
};

#[derive(Clone, Copy)]
pub struct SequenceReset {
    pub gap_fill: bool,
    pub new_seq_no: u64,
}

impl SequenceReset {
    pub const MSG_TYPE: &str = "4";
}

impl OutboundMessage for SequenceReset {
    fn write(&self, msg: &mut Message) {
        msg.set(GAP_FILL_FLAG, self.gap_fill);
        msg.set(NEW_SEQ_NO, self.new_seq_no);
        #[allow(clippy::expect_used)]
        let sending_time: Timestamp = msg.header().get(SENDING_TIME).expect(
            "sending time should always be present due to previously having validated message",
        );
        msg.header_mut().set(ORIG_SENDING_TIME, sending_time);
        msg.header_mut().set(POSS_DUP_FLAG, true);
    }

    fn message_type(&self) -> &str {
        Self::MSG_TYPE
    }
}
