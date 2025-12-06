use crate::config::SessionConfig;
use crate::error::{CompIdType, MessageVerificationError};
use hotfix_message::field_types::Timestamp;
use hotfix_message::fix44::{ORIG_SENDING_TIME, POSS_DUP_FLAG, SENDING_TIME};
use hotfix_message::message::Message;
use hotfix_message::{Part, fix44};
use std::cmp::Ordering;
use tracing::error;

/// Maximum allowed difference in seconds between SendingTime and current time
const SENDING_TIME_THRESHOLD: u64 = 120;

pub(crate) fn verify_message(
    message: &Message,
    config: &SessionConfig,
    expected_seq_number: u64,
) -> Result<(), MessageVerificationError> {
    check_begin_string(message, config.begin_string.as_str())?;
    let actual_seq_number: u64 = message.header().get(fix44::MSG_SEQ_NUM).unwrap_or_default();

    // our TargetCompId is always the same as the expected SenderCompId for them
    let expected_sender_comp_id: &str = config.target_comp_id.as_str();
    check_sender_comp_id(message, actual_seq_number, expected_sender_comp_id)?;

    // our SenderCompId is always the same as the expected TargetCompId for them
    let expected_target_comp_id: &str = config.sender_comp_id.as_str();
    check_target_comp_id(message, actual_seq_number, expected_target_comp_id)?;

    // check SendingTime and OrigSendingTime
    let sending_time = check_sending_time(message, actual_seq_number)?;
    let possible_duplicate = message.header().get::<bool>(POSS_DUP_FLAG).unwrap_or(false);
    if possible_duplicate {
        check_original_sending_time(message, actual_seq_number, sending_time)?;
    }

    check_sequence_number(actual_seq_number, expected_seq_number, possible_duplicate)?;

    Ok(())
}

fn check_begin_string(
    message: &Message,
    expected_begin_string: &str,
) -> Result<(), MessageVerificationError> {
    let begin_string: &str = message.header().get(fix44::BEGIN_STRING).unwrap_or("");
    if begin_string != expected_begin_string {
        return Err(MessageVerificationError::IncorrectBeginString(
            begin_string.to_string(),
        ));
    }

    Ok(())
}

fn check_sending_time(
    message: &Message,
    sequence_number: u64,
) -> Result<Timestamp, MessageVerificationError> {
    // Validate SendingTime presence
    let sending_time = match message.header().get::<Timestamp>(SENDING_TIME) {
        Ok(st) => st,
        Err(_) => {
            return Err(MessageVerificationError::SendingTimeMissing {
                msg_seq_num: sequence_number,
            });
        }
    };

    // Validate SendingTime is within threshold
    let now = Timestamp::utc_now();
    if let (Some(sending_chrono), Some(now_chrono)) =
        (sending_time.to_chrono_utc(), now.to_chrono_utc())
    {
        let diff = if sending_chrono > now_chrono {
            sending_chrono - now_chrono
        } else {
            now_chrono - sending_chrono
        };

        if diff.num_seconds() > SENDING_TIME_THRESHOLD as i64 {
            return Err(MessageVerificationError::SendingTimeAccuracyIssue {
                msg_seq_num: sequence_number,
            });
        }
    }

    Ok(sending_time)
}

fn check_original_sending_time(
    message: &Message,
    sequence_number: u64,
    sending_time: Timestamp,
) -> Result<(), MessageVerificationError> {
    match message.header().get::<Timestamp>(ORIG_SENDING_TIME) {
        Ok(original_sending_time) => {
            if original_sending_time > sending_time {
                return Err(
                    MessageVerificationError::OriginalSendingTimeAfterSendingTime {
                        msg_seq_num: sequence_number,
                        original_sending_time,
                        sending_time,
                    },
                );
            }
        }
        Err(err) => {
            error!(error = debug(err), "original sending time is missing");
            return Err(MessageVerificationError::OriginalSendingTimeMissing {
                msg_seq_num: sequence_number,
            });
        }
    }

    Ok(())
}

fn check_sender_comp_id(
    message: &Message,
    sequence_number: u64,
    expected_comp_id: &str,
) -> Result<(), MessageVerificationError> {
    let actual_sender_comp_id: &str = message.header().get(fix44::SENDER_COMP_ID).unwrap_or("");
    if actual_sender_comp_id != expected_comp_id {
        return Err(MessageVerificationError::IncorrectCompId {
            comp_id: actual_sender_comp_id.to_string(),
            comp_id_type: CompIdType::Sender,
            msg_seq_num: sequence_number,
        });
    }

    Ok(())
}

fn check_sequence_number(
    actual_seq_number: u64,
    expected_seq_number: u64,
    possible_duplicate: bool,
) -> Result<(), MessageVerificationError> {
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

fn check_target_comp_id(
    message: &Message,
    msg_seq_num: u64,
    expected_comp_id: &str,
) -> Result<(), MessageVerificationError> {
    let actual_target_comp_id: &str = message.header().get(fix44::TARGET_COMP_ID).unwrap_or("");
    if actual_target_comp_id != expected_comp_id {
        return Err(MessageVerificationError::IncorrectCompId {
            comp_id: actual_target_comp_id.to_string(),
            comp_id_type: CompIdType::Target,
            msg_seq_num,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Message, MessageVerificationError, SessionConfig, verify_message};
    use crate::error::CompIdType;
    use hotfix_message::field_types::Timestamp;
    use hotfix_message::{Part, fix44};

    fn build_test_config() -> SessionConfig {
        SessionConfig {
            begin_string: "FIX.4.4".to_string(),
            sender_comp_id: "SENDER".to_string(),
            target_comp_id: "TARGET".to_string(),
            data_dictionary_path: None,
            connection_host: "localhost".to_string(),
            connection_port: 9999,
            tls_config: None,
            heartbeat_interval: 0,
            logon_timeout: 0,
            logout_timeout: 0,
            reconnect_interval: 0,
            reset_on_logon: false,
            schedule: None,
        }
    }

    fn build_test_message(
        begin_string: &str,
        sender_comp_id: &str,
        target_comp_id: &str,
        seq_num: u64,
    ) -> Message {
        let mut msg = Message::new(begin_string, "D");
        msg.set(fix44::SENDER_COMP_ID, sender_comp_id);
        msg.set(fix44::TARGET_COMP_ID, target_comp_id);
        msg.set(fix44::MSG_SEQ_NUM, seq_num);
        msg.set(fix44::SENDING_TIME, Timestamp::utc_now());
        msg
    }

    #[test]
    fn test_verify_message_happy_path() {
        let config = build_test_config();
        let msg = build_test_message("FIX.4.4", "TARGET", "SENDER", 42);

        let result = verify_message(&msg, &config, 42);

        assert!(result.is_ok());
    }

    #[test]
    fn test_incorrect_begin_string() {
        let config = build_test_config();
        let msg = build_test_message("FIX.4.2", "TARGET", "SENDER", 42);

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::IncorrectBeginString(_))
        ));
        if let Err(MessageVerificationError::IncorrectBeginString(begin_string)) = result {
            assert_eq!(begin_string, "FIX.4.2");
        }
    }

    #[test]
    fn test_incorrect_sender_comp_id() {
        let config = build_test_config();
        let msg = build_test_message("FIX.4.4", "WRONG_SENDER", "SENDER", 42);

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::IncorrectCompId {
                comp_id_type: CompIdType::Sender,
                ..
            })
        ));
        if let Err(MessageVerificationError::IncorrectCompId {
            comp_id,
            comp_id_type,
            msg_seq_num,
        }) = result
        {
            assert_eq!(comp_id, "WRONG_SENDER");
            assert!(matches!(comp_id_type, CompIdType::Sender));
            assert_eq!(msg_seq_num, 42);
        }
    }

    #[test]
    fn test_incorrect_target_comp_id() {
        let config = build_test_config();
        let msg = build_test_message("FIX.4.4", "TARGET", "WRONG_TARGET", 42);

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::IncorrectCompId {
                comp_id_type: CompIdType::Target,
                ..
            })
        ));
        if let Err(MessageVerificationError::IncorrectCompId {
            comp_id,
            comp_id_type,
            msg_seq_num,
        }) = result
        {
            assert_eq!(comp_id, "WRONG_TARGET");
            assert!(matches!(comp_id_type, CompIdType::Target));
            assert_eq!(msg_seq_num, 42);
        }
    }

    #[test]
    fn test_seq_number_too_low() {
        let config = build_test_config();
        let msg = build_test_message("FIX.4.4", "TARGET", "SENDER", 40);

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::SeqNumberTooLow { .. })
        ));
        if let Err(MessageVerificationError::SeqNumberTooLow {
            expected,
            actual,
            possible_duplicate,
        }) = result
        {
            assert_eq!(expected, 42);
            assert_eq!(actual, 40);
            assert!(!possible_duplicate);
        }
    }

    #[test]
    fn test_seq_number_too_low_with_poss_dup_flag() {
        let config = build_test_config();
        let mut msg = build_test_message("FIX.4.4", "TARGET", "SENDER", 40);
        msg.header_mut().set(fix44::POSS_DUP_FLAG, true);
        msg.header_mut()
            .set(fix44::ORIG_SENDING_TIME, Timestamp::utc_now());

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::SeqNumberTooLow { .. })
        ));
        if let Err(MessageVerificationError::SeqNumberTooLow {
            expected,
            actual,
            possible_duplicate,
        }) = result
        {
            assert_eq!(expected, 42);
            assert_eq!(actual, 40);
            assert!(possible_duplicate);
        }
    }

    #[test]
    fn test_seq_number_too_high() {
        let config = build_test_config();
        let msg = build_test_message("FIX.4.4", "TARGET", "SENDER", 50);

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::SeqNumberTooHigh { .. })
        ));
        if let Err(MessageVerificationError::SeqNumberTooHigh { expected, actual }) = result {
            assert_eq!(expected, 42);
            assert_eq!(actual, 50);
        }
    }

    #[test]
    fn test_poss_dup_flag_missing_orig_sending_time() {
        let config = build_test_config();
        let mut msg = build_test_message("FIX.4.4", "TARGET", "SENDER", 42);
        msg.header_mut().set(fix44::POSS_DUP_FLAG, true);
        // Don't set OrigSendingTime

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::OriginalSendingTimeMissing { .. })
        ));
        if let Err(MessageVerificationError::OriginalSendingTimeMissing { msg_seq_num }) = result {
            assert_eq!(msg_seq_num, 42);
        }
    }

    #[test]
    fn test_poss_dup_flag_with_valid_orig_sending_time() {
        let config = build_test_config();
        let mut msg = build_test_message("FIX.4.4", "TARGET", "SENDER", 42);

        let orig_time = Timestamp::utc_now();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let sending_time = Timestamp::utc_now();

        msg.header_mut().set(fix44::POSS_DUP_FLAG, true);
        msg.header_mut().set(fix44::ORIG_SENDING_TIME, orig_time);
        msg.header_mut().pop(fix44::SENDING_TIME);
        msg.header_mut().set(fix44::SENDING_TIME, sending_time);

        let result = verify_message(&msg, &config, 42);

        assert!(result.is_ok());
    }

    #[test]
    fn test_orig_sending_time_after_sending_time() {
        let config = build_test_config();
        let mut msg = build_test_message("FIX.4.4", "TARGET", "SENDER", 42);

        let sending_time = Timestamp::utc_now();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let orig_time = Timestamp::utc_now();

        msg.header_mut().set(fix44::POSS_DUP_FLAG, true);
        msg.header_mut().set(fix44::ORIG_SENDING_TIME, orig_time);
        msg.header_mut().pop(fix44::SENDING_TIME);
        msg.header_mut().set(fix44::SENDING_TIME, sending_time);

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::OriginalSendingTimeAfterSendingTime { .. })
        ));
        if let Err(MessageVerificationError::OriginalSendingTimeAfterSendingTime {
            msg_seq_num,
            original_sending_time,
            sending_time: st,
        }) = result
        {
            assert_eq!(msg_seq_num, 42);
            assert!(original_sending_time > st);
        }
    }

    #[test]
    fn test_poss_dup_flag_with_equal_timestamps() {
        let config = build_test_config();
        let mut msg = build_test_message("FIX.4.4", "TARGET", "SENDER", 42);

        let timestamp = Timestamp::utc_now();

        msg.header_mut().set(fix44::POSS_DUP_FLAG, true);
        msg.header_mut()
            .set(fix44::ORIG_SENDING_TIME, timestamp.clone());
        msg.header_mut().pop(fix44::SENDING_TIME);
        msg.header_mut().set(fix44::SENDING_TIME, timestamp);

        let result = verify_message(&msg, &config, 42);

        // equal timestamps should be valid (orig <= sending)
        assert!(result.is_ok());
    }

    #[test]
    fn test_missing_begin_string() {
        let config = build_test_config();
        let mut msg = Message::new("FIX.4.4", "D");
        msg.set(fix44::SENDER_COMP_ID, "TARGET");
        msg.set(fix44::TARGET_COMP_ID, "SENDER");
        msg.set(fix44::MSG_SEQ_NUM, 42u64);
        msg.set(fix44::SENDING_TIME, Timestamp::utc_now());

        // remove begin string, which is automatically added by `Message::new`
        msg.header_mut().pop(fix44::BEGIN_STRING);

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::IncorrectBeginString(_))
        ));
    }

    #[test]
    fn test_missing_sender_comp_id() {
        let config = build_test_config();
        let mut msg = Message::new("FIX.4.4", "D");
        msg.set(fix44::TARGET_COMP_ID, "SENDER");
        msg.set(fix44::MSG_SEQ_NUM, 42u64);
        msg.set(fix44::SENDING_TIME, Timestamp::utc_now());

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::IncorrectCompId {
                comp_id_type: CompIdType::Sender,
                ..
            })
        ));
    }

    #[test]
    fn test_missing_target_comp_id() {
        let config = build_test_config();
        let mut msg = Message::new("FIX.4.4", "D");
        msg.set(fix44::SENDER_COMP_ID, "TARGET");
        msg.set(fix44::MSG_SEQ_NUM, 42u64);
        msg.set(fix44::SENDING_TIME, Timestamp::utc_now());

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::IncorrectCompId {
                comp_id_type: CompIdType::Target,
                ..
            })
        ));
    }

    #[test]
    fn test_missing_seq_number() {
        let config = build_test_config();
        let mut msg = Message::new("FIX.4.4", "D");
        msg.set(fix44::SENDER_COMP_ID, "TARGET");
        msg.set(fix44::TARGET_COMP_ID, "SENDER");
        msg.set(fix44::SENDING_TIME, Timestamp::utc_now());

        let result = verify_message(&msg, &config, 42);

        // missing seq num defaults to 0, which will be too low
        assert!(matches!(
            result,
            Err(MessageVerificationError::SeqNumberTooLow { .. })
        ));
    }

    #[test]
    fn test_seq_number_zero_when_expecting_one() {
        let config = build_test_config();
        let msg = build_test_message("FIX.4.4", "TARGET", "SENDER", 0);

        let result = verify_message(&msg, &config, 1);

        assert!(matches!(
            result,
            Err(MessageVerificationError::SeqNumberTooLow { .. })
        ));
    }

    #[test]
    fn test_first_message_with_seq_num_one() {
        let config = build_test_config();
        let msg = build_test_message("FIX.4.4", "TARGET", "SENDER", 1);

        let result = verify_message(&msg, &config, 1);

        assert!(result.is_ok());
    }

    #[test]
    fn test_verification_order_begin_string_checked_first() {
        let config = build_test_config();
        // wrong begin string AND wrong seq num - begin string error should come first
        let msg = build_test_message("FIX.4.2", "TARGET", "SENDER", 100);

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::IncorrectBeginString(_))
        ));
    }

    #[test]
    fn test_verification_order_sender_comp_id_checked_before_target() {
        let config = build_test_config();
        // wrong sender and wrong target - sender error should come first
        let msg = build_test_message("FIX.4.4", "WRONG_SENDER", "WRONG_TARGET", 42);

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::IncorrectCompId {
                comp_id_type: CompIdType::Sender,
                ..
            })
        ));
    }

    #[test]
    fn test_missing_sending_time() {
        let config = build_test_config();
        let mut msg = Message::new("FIX.4.4", "D");
        msg.set(fix44::SENDER_COMP_ID, "TARGET");
        msg.set(fix44::TARGET_COMP_ID, "SENDER");
        msg.set(fix44::MSG_SEQ_NUM, 42u64);

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::SendingTimeMissing { .. })
        ));
        if let Err(MessageVerificationError::SendingTimeMissing { msg_seq_num }) = result {
            assert_eq!(msg_seq_num, 42);
        }
    }

    #[test]
    fn test_sending_time_too_far_in_past() {
        use chrono::Duration;

        let config = build_test_config();
        let mut msg = Message::new("FIX.4.4", "D");
        msg.set(fix44::SENDER_COMP_ID, "TARGET");
        msg.set(fix44::TARGET_COMP_ID, "SENDER");
        msg.set(fix44::MSG_SEQ_NUM, 42u64);

        // set sending time to 121 seconds in the past (beyond the threshold)
        let now = chrono::Utc::now();
        let past_time = now - Duration::seconds(121);
        let past_timestamp: Timestamp = past_time.naive_utc().into();
        msg.set(fix44::SENDING_TIME, past_timestamp);

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::SendingTimeAccuracyIssue { .. })
        ));
        if let Err(MessageVerificationError::SendingTimeAccuracyIssue { msg_seq_num }) = result {
            assert_eq!(msg_seq_num, 42);
        }
    }

    #[test]
    fn test_sending_time_too_far_in_future() {
        use chrono::Duration;

        let config = build_test_config();
        let mut msg = Message::new("FIX.4.4", "D");
        msg.set(fix44::SENDER_COMP_ID, "TARGET");
        msg.set(fix44::TARGET_COMP_ID, "SENDER");
        msg.set(fix44::MSG_SEQ_NUM, 42u64);

        // set sending time to 121 seconds in the future (beyond the threshold)
        let now = chrono::Utc::now();
        let future_time = now + Duration::seconds(121);
        let future_timestamp: Timestamp = future_time.naive_utc().into();
        msg.set(fix44::SENDING_TIME, future_timestamp);

        let result = verify_message(&msg, &config, 42);

        assert!(matches!(
            result,
            Err(MessageVerificationError::SendingTimeAccuracyIssue { .. })
        ));
        if let Err(MessageVerificationError::SendingTimeAccuracyIssue { msg_seq_num }) = result {
            assert_eq!(msg_seq_num, 42);
        }
    }

    #[test]
    fn test_sending_time_at_threshold_boundary() {
        use chrono::Duration;

        let config = build_test_config();
        let mut msg = Message::new("FIX.4.4", "D");
        msg.set(fix44::SENDER_COMP_ID, "TARGET");
        msg.set(fix44::TARGET_COMP_ID, "SENDER");
        msg.set(fix44::MSG_SEQ_NUM, 42u64);

        // set sending time to exactly 120 seconds in the past (at the threshold)
        let now = chrono::Utc::now();
        let boundary_time = now - Duration::seconds(120);
        let boundary_timestamp: Timestamp = boundary_time.naive_utc().into();
        msg.set(fix44::SENDING_TIME, boundary_timestamp);

        let result = verify_message(&msg, &config, 42);

        assert!(result.is_ok());
    }

    #[test]
    fn test_sending_time_within_threshold() {
        use chrono::Duration;

        let config = build_test_config();
        let mut msg = Message::new("FIX.4.4", "D");
        msg.set(fix44::SENDER_COMP_ID, "TARGET");
        msg.set(fix44::TARGET_COMP_ID, "SENDER");
        msg.set(fix44::MSG_SEQ_NUM, 42u64);

        // set sending time to 60 seconds in the past (within the threshold)
        let now = chrono::Utc::now();
        let valid_time = now - Duration::seconds(60);
        let valid_timestamp: Timestamp = valid_time.naive_utc().into();
        msg.set(fix44::SENDING_TIME, valid_timestamp);

        let result = verify_message(&msg, &config, 42);

        assert!(result.is_ok());
    }
}
