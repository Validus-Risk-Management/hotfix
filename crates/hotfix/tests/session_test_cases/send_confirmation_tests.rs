use crate::common::actions::when;
use crate::common::assertions::then;
use crate::common::cleanup::finally;
use crate::common::setup::given_an_active_session;
use crate::common::test_messages::TestMessage;
use hotfix::message::{InboundMessage, OutboundMessage};
use hotfix::session::SendOutcome;

#[tokio::test]
async fn test_send_returns_sequence_number() {
    let (session, mut counterparty) = given_an_active_session().await;

    // Send a message and verify we get a SendOutcome::Sent with the correct sequence number
    let outcome = when(&session)
        .sends_message_with_confirmation(TestMessage::dummy_new_order_single())
        .await
        .expect("message should be sent successfully");

    // The sequence number should be 2 (1 is used for logon)
    match outcome {
        SendOutcome::Sent { sequence_number } => {
            assert_eq!(
                sequence_number, 2,
                "First app message should have sequence number 2"
            );
        }
        SendOutcome::Dropped => {
            panic!("Message should not have been dropped");
        }
    }

    // Verify counterparty received the message
    then(&mut counterparty)
        .receives(|msg| {
            let parsed = TestMessage::parse(msg);
            assert_eq!(parsed.message_type(), "D");
        })
        .await;

    finally(&session, &mut counterparty).disconnect().await;
}

#[tokio::test]
async fn test_send_multiple_messages_returns_sequential_sequence_numbers() {
    let (session, mut counterparty) = given_an_active_session().await;

    // Send first message
    let outcome1 = when(&session)
        .sends_message_with_confirmation(TestMessage::dummy_new_order_single())
        .await
        .expect("first message should be sent");

    // Send second message
    let outcome2 = when(&session)
        .sends_message_with_confirmation(TestMessage::dummy_execution_report())
        .await
        .expect("second message should be sent");

    // Verify sequence numbers are sequential
    match (outcome1, outcome2) {
        (
            SendOutcome::Sent {
                sequence_number: seq1,
            },
            SendOutcome::Sent {
                sequence_number: seq2,
            },
        ) => {
            assert_eq!(seq1, 2, "First message should have sequence number 2");
            assert_eq!(seq2, 3, "Second message should have sequence number 3");
        }
        _ => panic!("Both messages should have been sent successfully"),
    }

    // Drain the received messages
    then(&mut counterparty).receives(|_| {}).await;
    then(&mut counterparty).receives(|_| {}).await;

    finally(&session, &mut counterparty).disconnect().await;
}
