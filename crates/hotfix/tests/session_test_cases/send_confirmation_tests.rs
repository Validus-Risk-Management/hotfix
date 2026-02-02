use crate::common::actions::when;
use crate::common::assertions::then;
use crate::common::cleanup::finally;
use crate::common::setup::{
    given_a_disconnected_session, given_an_active_session,
    given_an_active_session_with_outbound_decision,
};
use crate::common::test_messages::TestMessage;
use hotfix::application::OutboundDecision;
use hotfix::message::{InboundMessage, OutboundMessage};
use hotfix::session::{SendError, SendOutcome};

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

#[tokio::test]
async fn test_send_forget_queues_message() {
    let (session, mut counterparty) = given_an_active_session().await;

    // Send a message using send_forget (no confirmation)
    when(&session)
        .sends_message_without_confirmation(TestMessage::dummy_new_order_single())
        .await
        .expect("message should be queued successfully");

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
async fn test_send_returns_disconnected_when_not_connected() {
    // Create a session without establishing a transport connection
    let disconnected_session = given_a_disconnected_session();

    // Try to send a message before any connection is established
    let result = disconnected_session
        .session_handle()
        .send(TestMessage::dummy_new_order_single())
        .await;

    // Should return Disconnected error since no transport connection exists
    match result {
        Err(SendError::Disconnected) => {
            // Expected - session has no transport connection
        }
        other => {
            panic!("Expected SendError::Disconnected, got {:?}", other);
        }
    }
}

#[tokio::test]
async fn test_send_returns_dropped_when_app_drops_message() {
    // Create an active session with an application configured to drop messages
    let (session, mut counterparty) =
        given_an_active_session_with_outbound_decision(OutboundDecision::Drop).await;

    // Send a message - should be dropped by the application
    let result = when(&session)
        .sends_message_with_confirmation(TestMessage::dummy_new_order_single())
        .await;

    // Verify we get SendOutcome::Dropped
    match result {
        Ok(SendOutcome::Dropped) => {
            // Expected - application chose to drop the message
        }
        other => {
            panic!("Expected SendOutcome::Dropped, got {:?}", other);
        }
    }

    finally(&session, &mut counterparty).disconnect().await;
}

#[tokio::test]
async fn test_send_returns_session_terminated_when_app_terminates() {
    // Create an active session with an application configured to terminate session
    let (session, mut counterparty) =
        given_an_active_session_with_outbound_decision(OutboundDecision::TerminateSession).await;

    // Send a message - should cause session termination
    let result = when(&session)
        .sends_message_with_confirmation(TestMessage::dummy_new_order_single())
        .await;

    // Verify we get SendError::SessionTerminated
    match result {
        Err(SendError::SessionTerminated) => {
            // Expected - application chose to terminate the session
        }
        other => {
            panic!("Expected SendError::SessionTerminated, got {:?}", other);
        }
    }

    // Session is terminated, so we just need to wait for disconnect
    counterparty
        .assert_disconnected_with_timeout(std::time::Duration::from_secs(5))
        .await;
}
