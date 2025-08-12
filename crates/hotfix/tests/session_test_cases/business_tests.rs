use crate::common::session_actions::SessionActions;
use crate::common::setup::given_an_active_session;
use crate::common::test_messages::TestMessage;
use hotfix::message::FixMessage;

#[tokio::test]
async fn test_new_order_single() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    // we send a new order to the counterparty and they receive it successfully
    session
        .when_message_is_sent(TestMessage::dummy_new_order_single())
        .await;
    mock_counterparty
        .then_receives(|msg| {
            let parsed = TestMessage::parse(msg);
            assert_eq!(parsed.message_type(), "D");
        })
        .await;

    mock_counterparty
        .when_message_is_sent(TestMessage::dummy_execution_report())
        .await;
    // TODO: we currently have no good way of asserting this message was received

    session.when_disconnect_is_requested().await;
    mock_counterparty.then_gets_disconnected().await;
}
