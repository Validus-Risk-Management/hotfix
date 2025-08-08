use crate::common::session_assertions::SessionAssertions;
use crate::common::setup::{HEARTBEAT_INTERVAL, setup};
use hotfix::session::Status;
use hotfix_message::Part;
use hotfix_message::fix44::MSG_TYPE;
use std::time::Duration;

async fn when_time_advances(duration: Duration) {
    tokio::time::advance(duration).await;
}

/// Tests the automatic heartbeat mechanism in an active FIX session:
/// 1. Establishes a session by exchanging logon messages with the counterparty
/// 2. Advances time beyond the configured heartbeat interval
/// 3. Verifies that a heartbeat message (type '0') is automatically sent
/// 4. Cleanly disconnects the session
///
/// This test ensures that the session maintains connectivity by sending
/// periodic heartbeat messages when no other messages are being exchanged,
/// as required by the FIX protocol to prevent timeout disconnections.
#[tokio::test(start_paused = true)]
async fn test_heartbeats() {
    let (session, mut mock_counterparty) = setup().await;

    // assert that a logon message is received (type 'A')
    mock_counterparty
        .then_receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;
    // counterparty responds with a logon to establish a happy session
    mock_counterparty.when_logon_is_sent().await;
    session.then_status_changes_to(Status::Active).await;

    // let's wait enough time for a heartbeat and assert that the heartbeat was sent
    when_time_advances(Duration::from_secs(HEARTBEAT_INTERVAL + 1)).await;
    mock_counterparty
        .then_receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "0"))
        .await;

    session.when_disconnected().await;
    mock_counterparty.then_disconnects().await;
}

/// Tests the peer timeout and disconnection mechanism:
/// 1. Establishes a session by exchanging logon messages
/// 2. Simulates peer inactivity by advancing time past the peer timeout threshold
/// 3. Verifies that a TestRequest message is sent to check peer responsiveness
/// 4. Continues to simulate peer silence and verifies automatic disconnection
///
/// This test ensures the session properly handles unresponsive peers by first
/// attempting to verify connectivity with a TestRequest, then disconnecting
/// if no response is received within the timeout period.
#[tokio::test(start_paused = true)]
async fn test_peer_timeout() {
    let (session, mut mock_counterparty) = setup().await;
    let peer_interval = (1.2 * HEARTBEAT_INTERVAL as f64) as u64 + 1;

    // assert that a logon message is received (type 'A')
    mock_counterparty
        .then_receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;
    session.then_status_changes_to(Status::AwaitingLogon).await;

    // counterparty responds with a logon to establish a happy session
    mock_counterparty.when_logon_is_sent().await;
    session.then_status_changes_to(Status::Active).await;

    // let's wait enough time for a heartbeat and assert that the heartbeat was sent
    when_time_advances(Duration::from_secs(HEARTBEAT_INTERVAL + 1)).await;
    mock_counterparty
        .then_receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "0"))
        .await;

    // we wait enough time for the peer deadline to pass
    when_time_advances(Duration::from_secs(peer_interval - HEARTBEAT_INTERVAL)).await;
    // a TestRequest (type '1') is sent to the counterparty
    mock_counterparty
        .then_receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "1"))
        .await;

    // we wait even longer and the counterparty never responds, so we disconnect from the counterparty
    when_time_advances(Duration::from_secs(peer_interval)).await;
    mock_counterparty.then_disconnects().await;
}
