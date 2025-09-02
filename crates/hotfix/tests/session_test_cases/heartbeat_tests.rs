use crate::common::actions::when;
use crate::common::assertions::then;
use crate::common::setup::{HEARTBEAT_INTERVAL, given_an_active_session};
use hotfix_message::Part;
use hotfix_message::fix44::MSG_TYPE;
use std::time::Duration;

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
    let (session, mut mock_counterparty) = given_an_active_session().await;

    // let's wait enough time for a heartbeat and assert that the heartbeat was sent
    when(Duration::from_secs(HEARTBEAT_INTERVAL + 1))
        .elapses()
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "0"))
        .await;

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
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
    let peer_interval = (1.2 * HEARTBEAT_INTERVAL as f64) as u64 + 1;
    let (_session, mut mock_counterparty) = given_an_active_session().await;

    // let's wait enough time for a heartbeat and assert that the heartbeat was sent
    when(Duration::from_secs(HEARTBEAT_INTERVAL + 1))
        .elapses()
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "0"))
        .await;

    // we wait enough time for the peer deadline to pass
    when(Duration::from_secs(peer_interval - HEARTBEAT_INTERVAL))
        .elapses()
        .await;
    // a TestRequest (type '1') is sent to the counterparty
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "1"))
        .await;

    // we wait even longer and the counterparty never responds, so we disconnect from the counterparty
    when(Duration::from_secs(peer_interval)).elapses().await;
    then(&mut mock_counterparty).gets_disconnected().await;
}
