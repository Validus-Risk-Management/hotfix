use crate::message::FixMessage;
use hotfix_message::Part;
use hotfix_message::message::Message;
use hotfix_message::session_fields::{BEGIN_SEQ_NO, END_SEQ_NO};

#[derive(Clone, Copy)]
pub struct ResendRequest {
    begin_seq_no: u64,
    end_seq_no: u64,
}

impl ResendRequest {
    pub fn new(begin: u64, end: u64) -> Self {
        Self {
            begin_seq_no: begin,
            end_seq_no: end,
        }
    }
}

impl FixMessage for ResendRequest {
    fn write(&self, msg: &mut Message) {
        msg.set(BEGIN_SEQ_NO, self.begin_seq_no);
        msg.set(END_SEQ_NO, self.end_seq_no);
    }

    fn message_type(&self) -> &str {
        "2"
    }

    fn parse(_message: &Message) -> Self {
        todo!()
    }
}
