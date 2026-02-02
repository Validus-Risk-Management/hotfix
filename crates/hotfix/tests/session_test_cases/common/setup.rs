use crate::common::actions::when;
use crate::common::assertions::then;
use crate::common::fakes::{FakeApplication, FakeCounterparty, SessionSpy};
use crate::common::test_messages::TestMessage;
use crate::session_test_cases::common::fakes::DisconnectedSession;
use hotfix::application::OutboundDecision;
use hotfix::config::SessionConfig;
use hotfix::session::InternalSessionRef;
use hotfix::session::Status;
use hotfix::store::in_memory::InMemoryMessageStore;
use hotfix_message::Part;
use hotfix_message::fix44::MSG_TYPE;

pub const HEARTBEAT_INTERVAL: u64 = 30;
pub const LOGON_TIMEOUT: u64 = 10;
pub const LOGOUT_TIMEOUT: u64 = 2;

pub const COUNTERPARTY_COMP_ID: &str = "dummy-acceptor";
pub const OUR_COMP_ID: &str = "dummy-initiator";

pub async fn given_a_connected_session() -> (SessionSpy, FakeCounterparty<TestMessage>) {
    let message_store = InMemoryMessageStore::default();
    given_a_connected_session_with_store(message_store).await
}

pub async fn given_a_connected_session_with_store(
    message_store: InMemoryMessageStore,
) -> (SessionSpy, FakeCounterparty<TestMessage>) {
    let config = create_session_config();
    let counterparty_config = create_counterparty_session_config(config.clone());

    let (message_tx, message_rx) = tokio::sync::mpsc::unbounded_channel();
    let session = InternalSessionRef::new(config, FakeApplication::new(message_tx), message_store)
        .expect("session to be created successfully");

    let session_spy = SessionSpy::new(session.clone().into(), message_rx);
    let mock_counterparty = FakeCounterparty::start(session.clone(), counterparty_config)
        .await
        .expect("failed to start FakeCounterparty");

    (session_spy, mock_counterparty)
}

/// Creates an active session with a configurable application.
pub async fn given_an_active_session_with_outbound_decision(
    decision: OutboundDecision,
) -> (SessionSpy, FakeCounterparty<TestMessage>) {
    let config = create_session_config();
    let counterparty_config = create_counterparty_session_config(config.clone());
    let message_store = InMemoryMessageStore::default();

    let (message_tx, message_rx) = tokio::sync::mpsc::unbounded_channel();
    let app = FakeApplication::with_outbound_decision(message_tx, decision);
    let session = InternalSessionRef::new(config, app, message_store)
        .expect("session to be created successfully");

    let mut session_spy = SessionSpy::new(session.clone().into(), message_rx);
    let mut mock_counterparty = FakeCounterparty::start(session.clone(), counterparty_config)
        .await
        .expect("failed to start FakeCounterparty");

    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;
    when(&mut mock_counterparty).sends_logon().await;
    then(&mut session_spy)
        .status_changes_to(Status::Active)
        .await;

    (session_spy, mock_counterparty)
}

/// Creates a session that has not yet established a transport connection.
/// This is useful for testing the Disconnected error case.
pub fn given_a_disconnected_session() -> DisconnectedSession {
    let config = create_session_config();
    let message_store = InMemoryMessageStore::default();

    let (message_tx, _message_rx) = tokio::sync::mpsc::unbounded_channel();
    let session_ref =
        InternalSessionRef::new(config, FakeApplication::new(message_tx), message_store)
            .expect("session to be created successfully");

    let session_handle = session_ref.clone().into();

    DisconnectedSession::new(session_ref, session_handle)
}

pub async fn given_an_active_session() -> (SessionSpy, FakeCounterparty<TestMessage>) {
    let (mut session, mut mock_counterparty) = given_a_connected_session().await;

    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "A"))
        .await;
    when(&mut mock_counterparty).sends_logon().await;
    then(&mut session).status_changes_to(Status::Active).await;

    (session, mock_counterparty)
}

pub fn create_session_config() -> SessionConfig {
    SessionConfig {
        begin_string: "FIX.4.4".to_string(),
        sender_comp_id: OUR_COMP_ID.to_string(),
        target_comp_id: COUNTERPARTY_COMP_ID.to_string(),
        data_dictionary_path: None,
        connection_host: "".to_string(),
        connection_port: 0,
        tls_config: None,
        heartbeat_interval: HEARTBEAT_INTERVAL,
        logon_timeout: LOGON_TIMEOUT,
        logout_timeout: LOGOUT_TIMEOUT,
        reconnect_interval: 30,
        reset_on_logon: false,
        schedule: None,
    }
}

/// Create a session configuration for the counterparty from our configuration.
pub fn create_counterparty_session_config(session_config: SessionConfig) -> SessionConfig {
    SessionConfig {
        sender_comp_id: session_config.target_comp_id.clone(),
        target_comp_id: session_config.sender_comp_id.clone(),
        ..session_config
    }
}
