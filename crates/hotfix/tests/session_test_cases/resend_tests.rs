use crate::common::actions::when;
use crate::common::assertions::{assert_msg_type, then};
use crate::common::setup::given_an_active_session;
use crate::common::test_messages::{
    TestMessage, build_execution_report_with_incorrect_body_length,
};
use hotfix::message::FixMessage;
use hotfix::session::Status;
use hotfix_message::FieldType;
use hotfix_message::fix44::MsgType;

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

    when(&session).requests_disconnect().await;
    then(&mut counterparty).gets_disconnected().await;
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
        .receives(|msg| assert_eq!(msg.message_type(), MsgType::ExecutionReport.to_string()))
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
            if let TestMessage::ExecutionReport { order_id, .. } = msg {
                assert_eq!(order_id, &new_report_order_id);
            } else {
                panic!("Unexpected message: {:?}", msg);
            }
        })
        .await;

    when(&session).requests_disconnect().await;
    then(&mut counterparty).gets_disconnected().await;
}
