use crate::common::actions::when;
use crate::common::assertions::{assert_msg_type, then};
use crate::common::setup::{COUNTERPARTY_COMP_ID, OUR_COMP_ID, given_an_active_session};
use crate::common::test_messages::{
    ExecutionReportWithInvalidField, TestMessage, build_execution_report_with_comp_id,
    build_execution_report_with_custom_msg_type,
    build_execution_report_with_incorrect_begin_string,
    build_execution_report_with_incorrect_body_length,
    build_execution_report_with_incorrect_orig_sending_time,
    build_execution_report_with_missing_orig_sending_time,
    build_execution_report_with_missing_sending_time,
    build_execution_report_with_sending_time_too_old,
};
use hotfix::session::Status;
use hotfix_message::Part;
use hotfix_message::fix44::{MsgType, SESSION_REJECT_REASON, SessionRejectReason};

/// Tests that when a counterparty sends a message containing an invalid/unrecognised field,
/// the session rejects the message by sending a Reject (MsgType=3) message back.
#[tokio::test]
async fn test_message_with_invalid_field_gets_rejected() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    when(&mut mock_counterparty)
        .sends_message(ExecutionReportWithInvalidField::default())
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::Reject))
        .await;

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests that when a counterparty sends a garbled message with an invalid body length,
/// the session silently ignores it and detects a sequence gap when the next valid message arrives.
#[tokio::test]
async fn test_garbled_message_with_invalid_target_comp_id_gets_ignored() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    // counterparty sends a message with invalid body length, which constitutes a garbled message
    let garbled_message_seq_num = mock_counterparty.next_target_sequence_number();
    let garbled_message =
        build_execution_report_with_incorrect_body_length(garbled_message_seq_num);
    when(&mut mock_counterparty)
        .sends_raw_message(garbled_message)
        .await;

    // they then send a valid message
    when(&mut mock_counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;

    // we then initiate a resend, having skipped the garbled message
    then(&mut mock_counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::ResendRequest))
        .await;
    then(&session)
        .status_changes_to(Status::AwaitingResend {
            begin: garbled_message_seq_num,
            end: garbled_message_seq_num + 1,
            attempts: 1,
        })
        .await;

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests that when a counterparty sends a message with an invalid BeginString,
/// the session logs out and disconnects.
#[tokio::test]
async fn test_message_with_invalid_begin_string() {
    let (_session, mut mock_counterparty) = given_an_active_session().await;

    // a message with invalid BeginString is sent by the counterparty
    let invalid_message = build_execution_report_with_incorrect_begin_string(
        mock_counterparty.next_target_sequence_number(),
    );
    when(&mut mock_counterparty)
        .sends_raw_message(invalid_message)
        .await;

    // then we log out and disconnect
    then(&mut mock_counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::Logout))
        .await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests that when a counterparty sends a message with an invalid TargetCompId,
/// the session sends a Reject (MsgType=3) and logs out and disconnects.
#[tokio::test]
async fn test_message_with_invalid_target_comp_id() {
    let (_session, mut mock_counterparty) = given_an_active_session().await;

    // a message with incorrect TargetCompId is sent by the counterparty
    let invalid_message = build_execution_report_with_comp_id(
        mock_counterparty.next_target_sequence_number(),
        COUNTERPARTY_COMP_ID,
        "WRONG_COMP_ID",
    );
    when(&mut mock_counterparty)
        .sends_raw_message(invalid_message)
        .await;

    // then we send a reject, log out and disconnect
    then(&mut mock_counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::Reject))
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::Logout))
        .await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests that when a counterparty sends a message with an invalid SenderCompId,
/// the session sends a Reject (MsgType=3) and logs out and disconnects.
#[tokio::test]
async fn test_message_with_invalid_sender_comp_id() {
    let (_session, mut mock_counterparty) = given_an_active_session().await;

    // a message with incorrect SenderCompId is sent by the counterparty
    let invalid_message = build_execution_report_with_comp_id(
        mock_counterparty.next_target_sequence_number(),
        "WRONG_COMP_ID",
        OUR_COMP_ID,
    );
    when(&mut mock_counterparty)
        .sends_raw_message(invalid_message)
        .await;

    // then we send a reject, log out and disconnect
    then(&mut mock_counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::Reject))
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::Logout))
        .await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests that when the counterparty sends a message with an invalid MsgType,
/// the session sends a Reject (MsgType=3) with the appropriate reject reason.
#[tokio::test]
async fn test_message_with_invalid_msg_type() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    // a message with invalid MsgType is sent by the counterparty
    let sequence_number = mock_counterparty.next_target_sequence_number();
    let invalid_message = build_execution_report_with_custom_msg_type(sequence_number, "ZZ");
    when(&mut mock_counterparty)
        .sends_raw_message(invalid_message)
        .await;

    // then we send a reject
    then(&mut mock_counterparty)
        .receives(|msg| {
            assert_msg_type(msg, MsgType::Reject);
            assert_eq!(msg.get::<u32>(SESSION_REJECT_REASON).unwrap(), 11);
        })
        .await;
    // our target sequence number should be incremented
    then(&session)
        .target_sequence_number_reaches(sequence_number)
        .await;

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests that a message with a sequence number lower than the expected one
/// causes the session to log out and disconnect the counterparty.
#[tokio::test]
async fn test_message_with_sequence_number_too_low() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    let sequence_number = mock_counterparty.next_target_sequence_number();
    when(&mut mock_counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;
    then(&session)
        .target_sequence_number_reaches(sequence_number)
        .await;

    // another message is sent, but due to a failure in the message store, it gets assigned the same sequence number
    mock_counterparty.delete_last_message_from_store();
    when(&mut mock_counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| {
            // we log them out
            assert_msg_type(msg, MsgType::Logout);
        })
        .await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests that a duplicate sequence number (too low) carrying PossDupFlag=Y is
/// treated as a safe retransmission and therefore ignored (no logout / reject),
/// and that subsequent in-sequence messages continue processing normally.
#[tokio::test]
async fn test_message_with_sequence_number_too_low_possdup_ignored() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    // A valid execution report is sent and processed normally
    let first_seq = mock_counterparty.next_target_sequence_number();
    when(&mut mock_counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;
    then(&session)
        .target_sequence_number_reaches(first_seq)
        .await;

    // The message is resent with PossDupFlag=Y
    // We expect the session to ignore this duplicate (no logout / no reject)
    when(&mut mock_counterparty)
        .resends_message(first_seq)
        .await;

    // A second message is sent, which should be accepted normally
    let second_seq = mock_counterparty.next_target_sequence_number();
    when(&mut mock_counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;
    then(&session)
        .target_sequence_number_reaches(second_seq)
        .await;

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests that a message with `OrigSendingTime` after `SendingTime` is rejected
/// with an appropriate rejection reason.
#[tokio::test]
async fn test_message_with_incorrect_orig_sending_time_is_rejected() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    // A valid execution report is sent and processed normally
    let seq_number = mock_counterparty.next_target_sequence_number();
    when(&mut mock_counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;
    then(&session)
        .target_sequence_number_reaches(seq_number)
        .await;

    // the same is resent with PossDupFlag=Y, but with OriginalSendingTime after SendingTime
    when(&mut mock_counterparty)
        .sends_raw_message(build_execution_report_with_incorrect_orig_sending_time(
            seq_number,
        ))
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| {
            assert_msg_type(msg, MsgType::Reject);
            assert_eq!(
                msg.get::<SessionRejectReason>(SESSION_REJECT_REASON)
                    .unwrap(),
                SessionRejectReason::SendingtimeAccuracyProblem
            );
        })
        .await;

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests that a message with missing `OrigSendingTime` is rejected.
///
/// `OrigSendingTime` is required when `PossDupFlag` is set to `Y`.
#[tokio::test]
async fn test_message_with_missing_orig_sending_time_is_rejected() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    // a valid execution report is sent and processed normally
    let seq_number = mock_counterparty.next_target_sequence_number();
    when(&mut mock_counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;
    then(&session)
        .target_sequence_number_reaches(seq_number)
        .await;

    // the same is resent with PossDupFlag=Y, but with OriginalSendingTime after SendingTime
    when(&mut mock_counterparty)
        .sends_raw_message(build_execution_report_with_missing_orig_sending_time(
            seq_number,
        ))
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| {
            assert_msg_type(msg, MsgType::Reject);
            assert_eq!(
                msg.get::<SessionRejectReason>(SESSION_REJECT_REASON)
                    .unwrap(),
                SessionRejectReason::RequiredTagMissing
            );
        })
        .await;

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests that a message with missing `SendingTime` is rejected.
///
/// `SendingTime` is a required field in all FIX messages.
#[tokio::test]
async fn test_message_with_missing_sending_time_is_rejected() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    // a message with missing SendingTime is sent by the counterparty
    let seq_number = mock_counterparty.next_target_sequence_number();
    when(&mut mock_counterparty)
        .sends_raw_message(build_execution_report_with_missing_sending_time(seq_number))
        .await;

    // then we send a reject with the appropriate reason
    then(&mut mock_counterparty)
        .receives(|msg| {
            assert_msg_type(msg, MsgType::Reject);
            assert_eq!(
                msg.get::<SessionRejectReason>(SESSION_REJECT_REASON)
                    .unwrap(),
                SessionRejectReason::SendingtimeAccuracyProblem
            );
        })
        .await;

    // our target sequence number should be incremented
    then(&session)
        .target_sequence_number_reaches(seq_number)
        .await;

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests that a message with `SendingTime` too far in the past is rejected.
///
/// Messages with `SendingTime` more than 120 seconds in the past should be rejected.
#[tokio::test]
async fn test_message_with_sending_time_too_old_is_rejected() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    // a message with SendingTime 121 seconds in the past is sent by the counterparty
    let seq_number = mock_counterparty.next_target_sequence_number();
    when(&mut mock_counterparty)
        .sends_raw_message(build_execution_report_with_sending_time_too_old(seq_number))
        .await;

    // then we send a reject with the appropriate reason
    then(&mut mock_counterparty)
        .receives(|msg| {
            assert_msg_type(msg, MsgType::Reject);
            assert_eq!(
                msg.get::<SessionRejectReason>(SESSION_REJECT_REASON)
                    .unwrap(),
                SessionRejectReason::SendingtimeAccuracyProblem
            );
        })
        .await;

    // our target sequence number should be incremented
    then(&session)
        .target_sequence_number_reaches(seq_number)
        .await;

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
}
