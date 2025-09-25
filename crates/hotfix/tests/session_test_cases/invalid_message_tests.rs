use crate::common::actions::when;
use crate::common::assertions::then;
use crate::common::setup::{COUNTERPARTY_COMP_ID, OUR_COMP_ID, given_an_active_session};
use crate::common::test_messages::{
    ExecutionReportWithInvalidField, TestMessage, build_execution_report_with_comp_id,
    build_execution_report_with_custom_msg_type,
    build_execution_report_with_incorrect_begin_string,
    build_execution_report_with_incorrect_body_length,
};
use hotfix::session::Status;
use hotfix_message::Part;
use hotfix_message::fix44::{MSG_TYPE, SESSION_REJECT_REASON};

/// Tests that when a counterparty sends a message containing an invalid/unrecognised field,
/// the session rejects the message by sending a Reject (MsgType=3) message back.
#[tokio::test]
async fn test_message_with_invalid_field_gets_rejected() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    when(&mut mock_counterparty)
        .sends_message(ExecutionReportWithInvalidField::default())
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "3"))
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
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "2"))
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
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "5"))
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
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "3"))
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "5"))
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
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "3"))
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "5"))
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
            assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "3");
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
