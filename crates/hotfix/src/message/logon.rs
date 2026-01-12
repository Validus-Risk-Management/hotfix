use crate::message::OutboundMessage;
use hotfix_message::message::Message;
use hotfix_message::session_fields::{
    ENCRYPT_METHOD, HEART_BT_INT, NEXT_EXPECTED_MSG_SEQ_NUM, RESET_SEQ_NUM_FLAG,
};
use hotfix_message::{FieldType, Part};

#[derive(Clone, Debug)]
pub struct Logon {
    encrypt_method: EncryptMethod,
    heartbeat_interval: u64,
    reset_seq_num_flag: ResetSeqNumFlag,
    next_expected_msg_seq_num: Option<u64>,
}

pub enum ResetSeqNumConfig {
    Reset,
    NoReset(Option<u64>),
}

impl Logon {
    pub fn new(heartbeat_interval: u64, reset_config: ResetSeqNumConfig) -> Self {
        let (reset_seq_num_flag, next_expected_msg_seq_num) = match reset_config {
            ResetSeqNumConfig::Reset => (ResetSeqNumFlag::Yes, None),
            ResetSeqNumConfig::NoReset(next) => (ResetSeqNumFlag::No, next),
        };
        Self {
            encrypt_method: EncryptMethod::None,
            heartbeat_interval,
            reset_seq_num_flag,
            next_expected_msg_seq_num,
        }
    }
}

impl OutboundMessage for Logon {
    fn write(&self, msg: &mut Message) {
        msg.set(ENCRYPT_METHOD, self.encrypt_method);
        msg.set(HEART_BT_INT, self.heartbeat_interval);
        msg.set(RESET_SEQ_NUM_FLAG, self.reset_seq_num_flag);

        if let Some(next) = self.next_expected_msg_seq_num {
            msg.set(NEXT_EXPECTED_MSG_SEQ_NUM, next);
        }
    }

    fn message_type(&self) -> &str {
        "A"
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, FieldType)]
pub enum EncryptMethod {
    /// Field variant '0'.
    #[hotfix(variant = "0")]
    None,

    /// Field variant '1'.
    #[hotfix(variant = "1")]
    Pkcs,

    /// Field variant '2'.
    #[hotfix(variant = "2")]
    Des,

    /// Field variant '3'.
    #[hotfix(variant = "3")]
    PkcsDes,

    /// Field variant '4'.
    #[hotfix(variant = "4")]
    PgpDes,

    /// Field variant '5'.
    #[hotfix(variant = "5")]
    PgpDesMd5,

    /// Field variant '6'.
    #[hotfix(variant = "6")]
    PemDesMd5,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, FieldType)]
pub enum ResetSeqNumFlag {
    /// Field variant 'Y'.
    #[hotfix(variant = "Y")]
    Yes,

    /// Field variant 'N'.
    #[hotfix(variant = "N")]
    No,
}
