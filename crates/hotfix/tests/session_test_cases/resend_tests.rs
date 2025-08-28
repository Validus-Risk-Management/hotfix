use crate::common::session_actions::SessionActions;
use crate::common::session_assertions::SessionAssertions;
use crate::common::setup::given_an_active_session;
use crate::common::test_messages::TestMessage;
use hotfix::session::Status;
use hotfix_message::Part;
use hotfix_message::fix44::MSG_TYPE;

#[tokio::test]
async fn test_message_sequence_number_too_high() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    // the counterparty previously sent an execution report which we missed
    mock_counterparty
        .when_previously_sent(TestMessage::dummy_execution_report())
        .await;

    // and they send a new report which we do receive
    mock_counterparty
        .when_message_is_sent(TestMessage::dummy_execution_report())
        .await;

    // we then ask them to resend the first message
    session.then_status_changes_to(Status::AwaitingResend).await;
    mock_counterparty
        .then_receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "2"))
        .await;

    session.when_disconnect_is_requested().await;
    mock_counterparty.then_gets_disconnected().await;
}
