use crate::common::actions::when;
use crate::common::assertions::then;
use crate::common::setup::given_an_active_session;
use crate::common::test_messages::TestMessage;
use hotfix::message::FixMessage;
use hotfix_message::{FieldType, fix44::MsgType};

#[tokio::test]
async fn test_new_order_single() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    // we send a new order to the counterparty and they receive it successfully
    when(&session)
        .sends_message(TestMessage::dummy_new_order_single())
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| {
            let parsed = TestMessage::parse(msg);
            assert_eq!(parsed.message_type(), MsgType::OrderSingle.to_string());
        })
        .await;

    when(&mut mock_counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;
    // TODO: we currently have no good way of asserting this message was received

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
}
