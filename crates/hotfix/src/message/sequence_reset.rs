use crate::message::FixMessage;
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

impl FixMessage for SequenceReset {
    fn write(&self, msg: &mut Message) {
        msg.set(GAP_FILL_FLAG, self.gap_fill);
        msg.set(NEW_SEQ_NO, self.new_seq_no);
        let sending_time: Timestamp = msg.header().get(SENDING_TIME).unwrap();
        msg.header_mut().set(ORIG_SENDING_TIME, sending_time);
        msg.header_mut().set(POSS_DUP_FLAG, true);
    }

    fn message_type(&self) -> &str {
        "4"
    }

    fn parse(_message: &Message) -> Self {
        todo!()
    }
}
