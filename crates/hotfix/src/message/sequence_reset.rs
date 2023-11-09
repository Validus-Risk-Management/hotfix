use crate::message::FixMessage;
use hotfix_message::field_types::Timestamp;
use hotfix_message::message::Message;
use hotfix_message::{fix44, Part};

#[derive(Clone)]
pub(crate) struct SequenceReset {
    pub(crate) gap_fill: bool,
    pub(crate) new_seq_no: u64,
}

impl FixMessage for SequenceReset {
    fn write(&self, msg: &mut Message) {
        msg.set(fix44::GAP_FILL_FLAG, self.gap_fill);
        msg.set(fix44::NEW_SEQ_NO, self.new_seq_no);
        let sending_time: Timestamp = msg.header().get(fix44::SENDING_TIME).unwrap();
        msg.header_mut().set(fix44::ORIG_SENDING_TIME, sending_time);
        msg.header_mut().set(fix44::POSS_DUP_FLAG, true);
    }

    fn message_type(&self) -> &str {
        "4"
    }

    fn parse(_message: &Message) -> Self {
        todo!()
    }
}
