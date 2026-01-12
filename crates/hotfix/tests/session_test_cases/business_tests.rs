use crate::common::actions::when;
use crate::common::assertions::then;
use crate::common::cleanup::finally;
use crate::common::setup::given_an_active_session;
use crate::common::test_messages::TestMessage;
use hotfix::message::{InboundMessage, OutboundMessage};
use hotfix_message::{FieldType, fix44::MsgType};

#[tokio::test]
async fn test_new_order_single() {
    let (mut session, mut counterparty) = given_an_active_session().await;

    // we send a new order to the counterparty and they receive it successfully
    when(&session)
        .sends_message(TestMessage::dummy_new_order_single())
        .await;
    then(&mut counterparty)
        .receives(|msg| {
            let parsed = TestMessage::parse(msg);
            assert_eq!(parsed.message_type(), MsgType::OrderSingle.to_string());
        })
        .await;

    when(&mut counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;
    then(&mut session)
        .receives(|msg| assert_eq!(msg.message_type(), MsgType::ExecutionReport.to_string()))
        .await;

    finally(&session, &mut counterparty).disconnect().await;
}
