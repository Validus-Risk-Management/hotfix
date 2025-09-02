use crate::common::actions::when;
use crate::common::assertions::then;
use crate::common::setup::given_an_active_session;
use crate::common::test_messages::TestMessage;
use hotfix::session::Status;
use hotfix_message::Part;
use hotfix_message::fix44::MSG_TYPE;

#[tokio::test]
async fn test_message_sequence_number_too_high() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    // the counterparty previously sent an execution report which we missed
    when(&mut mock_counterparty)
        .has_previously_sent(TestMessage::dummy_execution_report())
        .await;

    // and they send a new report which we do receive
    when(&mut mock_counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;

    // we then ask them to resend the first message
    then(&session)
        .status_changes_to(Status::AwaitingResend)
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "2"))
        .await;

    // the first message is the logon message, which doesn't need to be resent
    when(&mut mock_counterparty).resends_message(2).await; // the missed message is resent
    when(&mut mock_counterparty).resends_message(3).await; // the second message is resent
    then(&session).status_changes_to(Status::Active).await;

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
}
