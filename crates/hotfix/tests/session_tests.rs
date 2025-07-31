use crate::common::mock_application::MockApplication;
use crate::common::mock_counterparty::MockCounterparty;
use crate::common::test_messages::TestMessage;
use hotfix::application::ApplicationRef;
use hotfix::config::SessionConfig;
use hotfix::session::SessionRef;
use hotfix::store::in_memory::InMemoryMessageStore;
use hotfix_message::Part;
use hotfix_message::fix44::MSG_TYPE;

mod common;

const HEARTBEAT_INTERVAL: u64 = 30;

#[tokio::test]
async fn test_happy_login_flow() {
    let (session, mut mock_counterparty) = setup().await;

    // assert that a logon message is received (type 'A')
    mock_counterparty
        .assert_next(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;

    session
        .disconnect("Test Session Finished".to_string())
        .await;
    mock_counterparty.assert_disconnected().await;
}

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
