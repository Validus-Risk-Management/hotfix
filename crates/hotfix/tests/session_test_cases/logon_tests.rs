use crate::common::session_actions::SessionActions;
use crate::common::session_assertions::SessionAssertions;
use crate::common::setup::{given_a_connected_session, given_a_connected_session_with_store};
use crate::common::test_messages::TestMessage;
use hotfix::session::Status;
use hotfix::store::MessageStore;
use hotfix::store::in_memory::InMemoryMessageStore;
use hotfix_message::Part;
use hotfix_message::fix44::MSG_TYPE;

/// Tests successful FIX session establishment via logon message exchange.
/// Verifies that a session sends a logon message, receives a response,
/// transitions to Active status, and disconnects cleanly.
#[tokio::test]
async fn test_happy_logon() {
    let (session, mut mock_counterparty) = given_a_connected_session().await;

    // assert that a logon message is received (type 'A')
    mock_counterparty
        .then_receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;
    session.then_status_changes_to(Status::AwaitingLogon).await;

    // counterparty responds with a logon to establish a happy session
    mock_counterparty.when_logon_is_sent().await;
    session.then_status_changes_to(Status::Active).await;

    session.when_disconnect_is_requested().await;
    mock_counterparty.then_gets_disconnected().await;
}

/// Tests that sending a non-logon message (execution report) in response to a logon
/// request results in immediate disconnection. This verifies protocol compliance
/// where the first message after connection must be a logon response.
#[tokio::test]
async fn test_non_logon_response_to_logon() {
    let (session, mut mock_counterparty) = given_a_connected_session().await;

    // assert that a logon message is received (type 'A')
    mock_counterparty
        .then_receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;
    session.then_status_changes_to(Status::AwaitingLogon).await;

    // counterparty sends an execution report without ever responding to our logon
    let dummy_report = TestMessage::dummy_execution_report();
    mock_counterparty.when_message_is_sent(dummy_report).await;

    // we disconnect them as a result
    mock_counterparty.then_gets_disconnected().await;
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
    mock_counterparty
        .then_receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;
    session.then_status_changes_to(Status::AwaitingLogon).await;

    // counterparty responds with a logon, but their sequence number is lower than what we expect, which is 5
    mock_counterparty.when_logon_is_sent().await;
    // the counterparty then receives a logout message (type '5') and gets disconnected
    mock_counterparty
        .then_receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "5"))
        .await;
    mock_counterparty.then_gets_disconnected().await;
}
