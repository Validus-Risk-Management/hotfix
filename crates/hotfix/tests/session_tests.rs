use crate::common::mock_application::MockApplication;
use crate::common::mock_counterparty::MockCounterparty;
use crate::common::test_messages::TestMessage;
use hotfix::application::ApplicationRef;
use hotfix::config::SessionConfig;
use hotfix::session::SessionRef;
use hotfix::store::in_memory::InMemoryMessageStore;

mod common;

#[tokio::test]
async fn test_happy_login_flow() {
    let session = create_session();
    let mock_counterparty = MockCounterparty::start(session).await;
    mock_counterparty.assert_message_count(1, 0.5).await;
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
