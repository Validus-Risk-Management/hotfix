use crate::common::actions::when;
use crate::common::assertions::then;
use crate::common::setup::{
    LOGON_TIMEOUT, given_a_connected_session, given_a_connected_session_with_store,
};
use crate::common::test_messages::TestMessage;
use hotfix::session::Status;
use hotfix::store::MessageStore;
use hotfix::store::in_memory::InMemoryMessageStore;
use hotfix_message::Part;
use hotfix_message::fix44::MSG_TYPE;
use std::time::Duration;

/// Tests successful FIX session establishment via logon message exchange.
/// Verifies that a session sends a logon message, receives a response,
/// transitions to Active status, and disconnects cleanly.
#[tokio::test]
async fn test_happy_logon() {
    let (session, mut mock_counterparty) = given_a_connected_session().await;

    // assert that a logon message is received (type 'A')
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;
    then(&session)
        .status_changes_to(Status::AwaitingLogon)
        .await;

    // counterparty responds with a logon to establish a happy session
    when(&mut mock_counterparty).sends_logon().await;
    then(&session).status_changes_to(Status::Active).await;

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests that sending a non-logon message (execution report) in response to a logon
/// request results in immediate disconnection. This verifies protocol compliance
/// where the first message after connection must be a logon response.
#[tokio::test]
async fn test_non_logon_response_to_logon() {
    let (session, mut mock_counterparty) = given_a_connected_session().await;

    // assert that a logon message is received (type 'A')
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;
    then(&session)
        .status_changes_to(Status::AwaitingLogon)
        .await;

    // counterparty sends an execution report without ever responding to our logon
    let dummy_report = TestMessage::dummy_execution_report();
    when(&mut mock_counterparty)
        .sends_message(dummy_report)
        .await;

    // we disconnect them as a result
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests the scenario where the counterparty responds to our Logon message
/// with a Logon whose sequence number is lower than what we expect.
///
/// This means that we think we received messages from them that they are not aware of.
/// It's an unrecoverable scenario without human intervention which should result in
/// a Logout message and disconnect.
#[tokio::test]
async fn test_logon_response_with_sequence_number_too_low() {
    // a session is created with an expected sequence number of 5 for the counterparty
    let mut message_store = InMemoryMessageStore::default();
    message_store.set_target_seq_number(5).await.unwrap();
    let (session, mut mock_counterparty) =
        given_a_connected_session_with_store(message_store).await;

    // assert that a logon message is received (type 'A')
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;
    then(&session)
        .status_changes_to(Status::AwaitingLogon)
        .await;

    // counterparty responds with a logon, but their sequence number is lower than what we expect, which is 5
    when(&mut mock_counterparty).sends_logon().await;
    // the counterparty then receives a logout message (type '5') and gets disconnected
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "5"))
        .await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests the scenario where the counterparty's logon response has a higher sequence number than expected.
///
/// This simulates a scenario where our session has missed a message from the counterparty
/// before the logon sequence completes.
#[tokio::test]
async fn test_logon_response_with_sequence_number_too_high() {
    let (session, mut mock_counterparty) = given_a_connected_session().await;

    // the counterparty previously sent an execution report which we missed
    let dummy_report = TestMessage::dummy_execution_report();
    when(&mut mock_counterparty)
        .has_previously_sent(dummy_report)
        .await;

    // assert that a logon message is received (type 'A')
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;
    then(&session)
        .status_changes_to(Status::AwaitingLogon)
        .await;

    // the counterparty responds with a logon with a sequence number that indicates a message we missed
    when(&mut mock_counterparty).sends_logon().await;
    // we then ask them to resend the message
    then(&session)
        .status_changes_to(Status::AwaitingResend)
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "2"))
        .await;

    // the counterparty then completes the resend sequence and the session transitions to Active
    when(&mut mock_counterparty).resends_message(1).await; // the missed message is resent
    when(&mut mock_counterparty).sends_gap_fill(2, 3).await; // the logon is gap filled
    then(&session).status_changes_to(Status::Active).await;

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests the scenario where the counterparty does not respond to our logon message
/// within the configured timeout.
///
/// This results in us disconnecting.
#[tokio::test(start_paused = true)]
async fn test_logon_timeout() {
    let (session, mut mock_counterparty) = given_a_connected_session().await;

    // assert that a logon message is received (type 'A')
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;
    then(&session)
        .status_changes_to(Status::AwaitingLogon)
        .await;

    // enough time elapses for the logon to timeout
    when(Duration::from_secs(LOGON_TIMEOUT)).elapses().await;

    then(&mut mock_counterparty).gets_disconnected().await;
}
