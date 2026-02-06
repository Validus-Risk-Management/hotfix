use crate::common::actions::when;
use crate::common::assertions::{assert_msg_type, then};
use crate::common::cleanup::finally;
use crate::common::setup::{HEARTBEAT_INTERVAL, given_an_active_session};
use crate::common::test_messages::{
    TestMessage, build_execution_report_with_incorrect_body_length, build_invalid_resend_request,
};
use hotfix::message::ResendRequest;
use hotfix::session::Status;
use hotfix_message::fix44::{GAP_FILL_FLAG, MSG_TYPE, MsgType, NEW_SEQ_NO, ORDER_ID};
use hotfix_message::{FieldType, Part};
use std::time::Duration;

#[tokio::test]
async fn test_message_sequence_number_too_high() {
    let (mut session, mut counterparty) = given_an_active_session().await;

    // the counterparty previously sent an execution report which we missed
    when(&mut counterparty)
        .has_previously_sent(TestMessage::dummy_execution_report())
        .await;

    // and they send a new report which we do receive
    when(&mut counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;

    // we then ask them to resend the first message
    then(&mut session)
        .status_changes_to(Status::AwaitingResend {
            begin: 2,
            end: 3,
            attempts: 1,
        })
        .await;
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::ResendRequest))
        .await;

    // the first message is the logon message, which doesn't need to be resent
    when(&mut counterparty).resends_message(2).await; // the missed message is resent
    when(&mut counterparty).resends_message(3).await; // the second message is resent
    then(&mut session).status_changes_to(Status::Active).await;

    finally(&session, &mut counterparty).disconnect().await;
}

/// Tests that when a counterparty repeatedly resends garbled messages that cannot be processed,
/// the session eventually terminates the connection after exceeding the maximum resend attempts threshold.
#[tokio::test]
async fn test_infinite_resend_requests_are_prevented() {
    let (mut session, mut counterparty) = given_an_active_session().await;

    // counterparty sends a message with invalid body length, which we skip as it's a garbled message
    let garbled_message_seq_num = counterparty.next_target_sequence_number();
    let garbled_message =
        build_execution_report_with_incorrect_body_length(garbled_message_seq_num);
    when(&mut counterparty)
        .sends_raw_message(garbled_message)
        .await;

    // they then send a valid message
    when(&mut counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;

    // we then initiate a resend, having skipped the garbled message
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::ResendRequest))
        .await;
    then(&mut session)
        .status_changes_to(Status::AwaitingResend {
            begin: garbled_message_seq_num,
            end: garbled_message_seq_num + 1,
            attempts: 1,
        })
        .await;

    // the counterparty attempts to resend twice more, but we are still unable to process the garbled message
    for attempts in 2..4 {
        when(&mut counterparty)
            .resends_message_without_modification(garbled_message_seq_num)
            .await;
        when(&mut counterparty)
            .resends_message(garbled_message_seq_num + 1)
            .await;
        then(&mut session)
            .status_changes_to(Status::AwaitingResend {
                begin: garbled_message_seq_num,
                end: garbled_message_seq_num + 1,
                attempts,
            })
            .await;
        then(&mut counterparty)
            .receives(|msg| assert_msg_type(msg, MsgType::ResendRequest))
            .await;
    }

    // they try a third time, which exceeds are attempts threshold, so the connection is terminated
    when(&mut counterparty)
        .resends_message_without_modification(garbled_message_seq_num)
        .await;
    when(&mut counterparty)
        .resends_message(garbled_message_seq_num + 1)
        .await;
    then(&mut counterparty).gets_disconnected().await;
}

/// Tests that when a counterparty resends a message we previously received,
/// the session ignores the resent message and does not increment the target sequence number.
#[tokio::test]
async fn test_resent_message_previously_received_is_ignored() {
    let (mut session, mut counterparty) = given_an_active_session().await;

    when(&mut counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;
    then(&mut session)
        .receives(|msg| {
            let msg_type: &str = msg.header().get(MSG_TYPE).unwrap();
            assert_eq!(msg_type, MsgType::ExecutionReport.to_string());
        })
        .await;
    then(&mut session).target_sequence_number_reaches(2).await;

    // they resend a message we previously received, which we ignore - not affecting future messages
    when(&mut counterparty).resends_message(2).await;

    // the counterparty then sends another report, and we assert this is the next message we receive
    let new_report_order_id = "xxx".to_string();
    when(&mut counterparty)
        .sends_message(TestMessage::dummy_execution_report_with_order_id(
            new_report_order_id.clone(),
        ))
        .await;
    then(&mut session)
        .receives(|msg| {
            let msg_type: &str = msg.header().get(MSG_TYPE).unwrap();
            assert_eq!(msg_type, MsgType::ExecutionReport.to_string());
            let order_id: &str = msg.get(ORDER_ID).unwrap();
            assert_eq!(order_id, &new_report_order_id);
        })
        .await;

    finally(&session, &mut counterparty).disconnect().await;
}

/// Tests that when a counterparty sends a resend request without the required field,
/// the session rejects the invalid message.
#[tokio::test]
async fn test_invalid_resend_request_gets_rejected() {
    // We run the test twice - once with an invalid BeginSeqNo and once with an invalid EndSeqNo.
    for (begin_seq_no, end_seq_no) in [(None, Some(2)), (Some(1), None)] {
        let (session, mut counterparty) = given_an_active_session().await;

        // build a resend request message missing the required BeginSeqNo (tag 7)
        let seq_num = counterparty.next_target_sequence_number();
        let invalid_resend_request =
            build_invalid_resend_request(seq_num, begin_seq_no, end_seq_no);
        when(&mut counterparty)
            .sends_raw_message(invalid_resend_request)
            .await;

        // the session should reject this invalid resend request
        then(&mut counterparty)
            .receives(|msg| assert_msg_type(msg, MsgType::Reject))
            .await;

        finally(&session, &mut counterparty).disconnect().await;
    }
}

/// Tests that when a counterparty requests a resend of both admin and business messages,
/// the session gap fills admin messages and resends business messages as expected.
#[tokio::test(start_paused = true)]
async fn test_resend_request_with_gap_fill_for_admin_messages() {
    let (session, mut counterparty) = given_an_active_session().await;

    // wait for a heartbeat to be sent automatically (this will be message sequence number 2)
    when(Duration::from_secs(HEARTBEAT_INTERVAL + 1))
        .elapses()
        .await;
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::Heartbeat))
        .await;

    // send an execution report from the session (this will be message sequence number 3)
    when(&session)
        .sends_message(TestMessage::dummy_execution_report())
        .await;
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::ExecutionReport))
        .await;

    // counterparty requests a resend of messages 2 and 3
    let resend_request = ResendRequest::new(2, 3);
    when(&mut counterparty).sends_message(resend_request).await;

    // the session should send a SequenceReset-GapFill for the heartbeat (message 2)
    then(&mut counterparty)
        .receives(|msg| {
            assert_msg_type(msg, MsgType::SequenceReset);
            assert_eq!(msg.get::<&str>(GAP_FILL_FLAG).unwrap(), "Y");
            // the gap fill's MsgSeqNum indicates the beginning of the gap
            assert_eq!(
                msg.header()
                    .get::<u64>(hotfix_message::fix44::MSG_SEQ_NUM)
                    .unwrap(),
                2
            );
            // NewSeqNo indicates the next sequence number after the gap
            assert_eq!(msg.get::<u64>(NEW_SEQ_NO).unwrap(), 3);
        })
        .await;

    // the session should resend the execution report (message 3)
    then(&mut counterparty)
        .receives(|msg| assert_msg_type(msg, MsgType::ExecutionReport))
        .await;

    finally(&session, &mut counterparty).disconnect().await;
}
