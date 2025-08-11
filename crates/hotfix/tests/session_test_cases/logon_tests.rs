use crate::common::session_actions::SessionActions;
use crate::common::session_assertions::SessionAssertions;
use crate::common::setup::given_a_connected_session;
use crate::common::test_messages::TestMessage;
use hotfix::session::Status;
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
