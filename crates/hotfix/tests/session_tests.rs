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

#[tokio::test]
async fn test_happy_login_flow() {
    let session = create_session();
    let mut mock_counterparty = MockCounterparty::start(session.clone()).await;

    // assert that a logon message is received (type 'A')
    mock_counterparty
        .assert_next(|msg| msg.header().get::<&str>(MSG_TYPE).unwrap() == "A")
        .await;

    session
        .disconnect("Test Session Finished".to_string())
        .await;
    mock_counterparty.assert_disconnected().await;
}

fn create_session() -> SessionRef<TestMessage> {
    let config = create_session_config();
    let application_ref = ApplicationRef::new(MockApplication {});
    let message_store = InMemoryMessageStore::default();

    SessionRef::new(config, application_ref, message_store)
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
        heartbeat_interval: 30,
        reconnect_interval: 30,
        reset_on_logon: false,
        schedule: None,
    }
}
