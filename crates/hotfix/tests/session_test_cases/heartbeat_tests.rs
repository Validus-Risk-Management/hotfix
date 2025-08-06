use crate::common::mock_application::MockApplication;
use crate::common::mock_counterparty::MockCounterparty;
use crate::common::session_assertions::SessionAssertions;
use crate::common::test_messages::TestMessage;
use hotfix::application::ApplicationRef;
use hotfix::config::SessionConfig;
use hotfix::session::{SessionRef, Status};
use hotfix::store::in_memory::InMemoryMessageStore;
use hotfix_message::Part;
use hotfix_message::fix44::MSG_TYPE;

const HEARTBEAT_INTERVAL: u64 = 30;

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
        .assert_next(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;
    // counterparty responds with a logon to establish a happy session
    mock_counterparty.send_logon().await;
    tokio::task::yield_now().await;

    // let's wait enough time for a heartbeat and assert that the heartbeat was sent
    tokio::time::advance(std::time::Duration::from_secs(HEARTBEAT_INTERVAL + 1)).await;
    mock_counterparty
        .assert_next(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "0"))
        .await;

    session
        .disconnect("Test Session Finished".to_string())
        .await;
    mock_counterparty.assert_disconnected().await;
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
        .assert_next(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;
    session.assert_status(Status::AwaitingLogon).await;

    // counterparty responds with a logon to establish a happy session
    mock_counterparty.send_logon().await;
    session.assert_status(Status::Active).await;

    // let's wait enough time for a heartbeat and assert that the heartbeat was sent
    tokio::time::advance(std::time::Duration::from_secs(HEARTBEAT_INTERVAL + 1)).await;
    mock_counterparty
        .assert_next(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "0"))
        .await;

    // we wait enough time for the peer deadline to pass
    tokio::time::advance(std::time::Duration::from_secs(
        peer_interval - HEARTBEAT_INTERVAL,
    ))
    .await;
    // a TestRequest (type '1') is sent to the counterparty
    mock_counterparty
        .assert_next(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "1"))
        .await;

    // we wait even longer and the counterparty never responds, so we disconnect from the counterparty
    tokio::time::advance(std::time::Duration::from_secs(peer_interval)).await;
    mock_counterparty.assert_disconnected().await;
}

async fn setup() -> (SessionRef<TestMessage>, MockCounterparty<TestMessage>) {
    let config = create_session_config();
    let counterparty_config = create_counterparty_session_config(config.clone());

    let application_ref = ApplicationRef::new(MockApplication {});
    let message_store = InMemoryMessageStore::default();

    let session = SessionRef::new(config, application_ref, message_store);
    let mock_counterparty = MockCounterparty::start(session.clone(), counterparty_config).await;

    (session, mock_counterparty)
}

fn create_session_config() -> SessionConfig {
    SessionConfig {
        begin_string: "FIX.4.4".to_string(),
        sender_comp_id: "dummy-initiator".to_string(),
        target_comp_id: "dummy-acceptor".to_string(),
        data_dictionary_path: None,
        connection_host: "".to_string(),
        connection_port: 0,
        tls_config: None,
        heartbeat_interval: HEARTBEAT_INTERVAL,
        reconnect_interval: 30,
        reset_on_logon: false,
        schedule: None,
    }
}

/// Create a session configuration for the counterparty from our configuration.
fn create_counterparty_session_config(session_config: SessionConfig) -> SessionConfig {
    SessionConfig {
        sender_comp_id: session_config.target_comp_id.clone(),
        target_comp_id: session_config.sender_comp_id.clone(),
        ..session_config
    }
}
