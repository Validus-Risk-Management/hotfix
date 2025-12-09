static ADMIN_TYPES: [&str; 7] = ["A", "0", "1", "2", "3", "4", "5"];

pub fn is_admin(message_type: &str) -> bool {
    ADMIN_TYPES.contains(&message_type)
}

use hotfix_message::Part;
use hotfix_message::field_types::Timestamp;
use hotfix_message::message::Message;
use hotfix_message::session_fields::{ORIG_SENDING_TIME, POSS_DUP_FLAG, SENDING_TIME};

/// Prepares a FIX message for resend per the FIX spec (PossDupFlag logic).
///
/// Behaviour:
/// - On first resend (no PossDupFlag Y / no OrigSendingTime):
///   * Move current SendingTime(52) to OrigSendingTime(122)
///   * Set SendingTime(52) to the current sending time (may be equal if clock granularity causes no change)
///   * Set PossDupFlag(43)=Y
/// - On subsequent resends (already marked possible duplicate and has OrigSendingTime):
///   * Refresh SendingTime(52) to current time (value may or may not differ from previous)
pub fn prepare_message_for_resend(msg: &mut Message) -> Result<(), &'static str> {
    let header = msg.header_mut();

    if header.get_raw(SENDING_TIME).is_none() {
        return Err("Missing SendingTime (52)");
    }

    let already_poss_dup = header.get::<bool>(POSS_DUP_FLAG).unwrap_or(false);
    let has_orig_sending_time = header.get_raw(ORIG_SENDING_TIME).is_some();

    if already_poss_dup && has_orig_sending_time {
        // Subsequent resend: refresh SendingTime only
        return if header.pop(SENDING_TIME).is_some() {
            header.set(SENDING_TIME, Timestamp::utc_now());
            Ok(())
        } else {
            Err("Failed to extract previous SendingTime")
        };
    }

    // First resend path
    if let Some(original_sending_time_field) = header.pop(SENDING_TIME) {
        let original_ts = Timestamp::parse(&original_sending_time_field.data)
            .ok_or("Invalid original SendingTime format")?;
        header.set(ORIG_SENDING_TIME, original_ts);
        header.set(SENDING_TIME, Timestamp::utc_now());
        header.set(POSS_DUP_FLAG, true);
        Ok(())
    } else {
        Err("Failed to extract original SendingTime")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hotfix_message::fix44;

    fn build_test_message() -> Message {
        let mut msg = Message::new("FIX.4.4", "D");
        msg.set(fix44::SENDER_COMP_ID, "SND");
        msg.set(fix44::TARGET_COMP_ID, b"TGT");
        msg.set(fix44::MSG_SEQ_NUM, 1u64);
        msg.set(fix44::SENDING_TIME, Timestamp::utc_now());
        msg
    }

    #[test]
    fn first_resend_sets_poss_dup_and_orig_sending_time() {
        let mut msg = build_test_message();
        prepare_message_for_resend(&mut msg).unwrap();
        let header = msg.header();
        assert!(
            header.get::<bool>(fix44::POSS_DUP_FLAG).unwrap(),
            "PossDupFlag must be set on first resend"
        );
        // Presence checks only (values may be equal or different depending on clock granularity)
        assert!(
            header.get_raw(fix44::ORIG_SENDING_TIME).is_some(),
            "OrigSendingTime must be present"
        );
        assert!(
            header.get_raw(fix44::SENDING_TIME).is_some(),
            "SendingTime must be present after resend"
        );
    }

    #[test]
    fn subsequent_resend_preserves_orig_sending_time() {
        let mut msg = build_test_message();
        prepare_message_for_resend(&mut msg).unwrap();
        let orig_first = msg
            .header()
            .get::<Timestamp>(fix44::ORIG_SENDING_TIME)
            .unwrap();
        let sending_first = msg.header().get::<Timestamp>(fix44::SENDING_TIME).unwrap();
        assert!(
            msg.header().get::<bool>(fix44::POSS_DUP_FLAG).unwrap(),
            "PossDupFlag must be set after first resend"
        );

        // Second resend
        prepare_message_for_resend(&mut msg).unwrap();
        let orig_second = msg
            .header()
            .get::<Timestamp>(fix44::ORIG_SENDING_TIME)
            .unwrap();
        let sending_second = msg.header().get::<Timestamp>(fix44::SENDING_TIME).unwrap();
        assert!(
            msg.header().get::<bool>(fix44::POSS_DUP_FLAG).unwrap(),
            "PossDupFlag must remain set on subsequent resends"
        );

        assert_eq!(
            orig_first, orig_second,
            "OrigSendingTime must remain constant across resends"
        );
        assert!(
            sending_first >= orig_first,
            "First resend SendingTime must be >= original"
        );
        assert!(
            sending_second >= sending_first,
            "Second resend SendingTime must be >= first resend SendingTime"
        );
    }
}
