use crate::config::SessionConfig;
use crate::error::{CompIdType, MessageVerificationError};
use hotfix_message::field_types::Timestamp;
use hotfix_message::fix44::{ORIG_SENDING_TIME, POSS_DUP_FLAG, SENDING_TIME};
use hotfix_message::message::Message;
use hotfix_message::{Part, fix44};
use std::cmp::Ordering;
use tracing::error;

pub(crate) fn verify_message(
    message: &Message,
    config: &SessionConfig,
    expected_seq_number: u64,
) -> Result<(), MessageVerificationError> {
    let begin_string: &str = message.header().get(fix44::BEGIN_STRING).unwrap_or("");
    if begin_string != config.begin_string.as_str() {
        return Err(MessageVerificationError::IncorrectBeginString(
            begin_string.to_string(),
        ));
    }

    let actual_seq_number: u64 = message.header().get(fix44::MSG_SEQ_NUM).unwrap_or_default();

    // our TargetCompId is always the same as the expected SenderCompId for them
    let expected_sender_comp_id: &str = config.target_comp_id.as_str();
    let actual_sender_comp_id: &str = message.header().get(fix44::SENDER_COMP_ID).unwrap_or("");
    if expected_sender_comp_id != actual_sender_comp_id {
        return Err(MessageVerificationError::IncorrectCompId {
            comp_id: actual_sender_comp_id.to_string(),
            comp_id_type: CompIdType::Sender,
            msg_seq_num: actual_seq_number,
        });
    }

    // our SenderCompId is always the same as the expected TargetCompId for them
    let expected_target_comp_id: &str = config.sender_comp_id.as_str();
    let actual_target_comp_id: &str = message.header().get(fix44::TARGET_COMP_ID).unwrap_or("");
    if expected_target_comp_id != actual_target_comp_id {
        return Err(MessageVerificationError::IncorrectCompId {
            comp_id: actual_target_comp_id.to_string(),
            comp_id_type: CompIdType::Target,
            msg_seq_num: actual_seq_number,
        });
    }

    let possible_duplicate = message.header().get::<bool>(POSS_DUP_FLAG).unwrap_or(false);
    if possible_duplicate {
        match message.header().get::<Timestamp>(ORIG_SENDING_TIME) {
            Ok(original_sending_time) => {
                if let Ok(sending_time) = message.header().get::<Timestamp>(SENDING_TIME) {
                    // TODO: check presence of sending time (see related test cases https://www.fixtrading.org/standards/fix-session-testcases-online/#scenario-2-receive-message-standard-header)
                    if original_sending_time > sending_time {
                        return Err(
                            MessageVerificationError::OriginalSendingTimeAfterSendingTime {
                                msg_seq_num: actual_seq_number,
                                original_sending_time,
                                sending_time,
                            },
                        );
                    }
                }
            }
            Err(err) => {
                error!(error = debug(err), "original sending time is missing");
                return Err(MessageVerificationError::OriginalSendingTimeMissing {
                    msg_seq_num: actual_seq_number,
                });
            }
        }
    }

    match actual_seq_number.cmp(&expected_seq_number) {
        Ordering::Greater => {
            return Err(MessageVerificationError::SeqNumberTooHigh {
                expected: expected_seq_number,
                actual: actual_seq_number,
            });
        }
        Ordering::Less => {
            return Err(MessageVerificationError::SeqNumberTooLow {
                expected: expected_seq_number,
                actual: actual_seq_number,
                possible_duplicate,
            });
        }
        _ => {}
    }

    Ok(())
}
