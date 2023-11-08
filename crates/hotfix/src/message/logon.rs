use hotfix_message::fix44;
use hotfix_message::message::Message;

use crate::message::FixMessage;

#[derive(Clone, Debug)]
pub struct Logon {
    encrypt_method: fix44::EncryptMethod,
    heartbeat_interval: u64,
    reset_seq_num_flag: fix44::ResetSeqNumFlag,
    next_expected_msg_seq_num: Option<u64>,
}

pub enum ResetSeqNumConfig {
    Reset,
    NoReset(Option<u64>),
}

impl Logon {
    pub fn new(heartbeat_interval: u64, reset_config: ResetSeqNumConfig) -> Self {
        let (reset_seq_num_flag, next_expected_msg_seq_num) = match reset_config {
            ResetSeqNumConfig::Reset => (fix44::ResetSeqNumFlag::Yes, None),
            ResetSeqNumConfig::NoReset(next) => (fix44::ResetSeqNumFlag::No, next),
        };
        Self {
            encrypt_method: fix44::EncryptMethod::None,
            heartbeat_interval,
            reset_seq_num_flag,
            next_expected_msg_seq_num,
        }
    }
}

impl FixMessage for Logon {
    fn write(&self, msg: &mut Message) {
        msg.set(fix44::ENCRYPT_METHOD, self.encrypt_method);
        msg.set(fix44::HEART_BT_INT, self.heartbeat_interval);
        msg.set(fix44::RESET_SEQ_NUM_FLAG, self.reset_seq_num_flag);

        if let Some(next) = self.next_expected_msg_seq_num {
            msg.set(fix44::NEXT_EXPECTED_MSG_SEQ_NUM, next);
        }
    }

    fn message_type(&self) -> &str {
        "A"
    }

    fn parse(_message: &Message) -> Self {
        todo!()
    }
}
