use crate::common::session_actions::SessionActions;
use crate::common::setup::given_an_active_session;
use hotfix::message::FixMessage;
use hotfix_message::dict::{FieldLocation, FixDatatype};
use hotfix_message::fix44::MSG_TYPE;
use hotfix_message::message::Message;
use hotfix_message::{HardCodedFixFieldDefinition, Part, fix44};

#[tokio::test]
#[should_panic]
async fn test_message_with_invalid_field_gets_rejected() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    mock_counterparty
        .when_message_is_sent(ExecutionReportWithInvalidField::default())
        .await;
    mock_counterparty
        .then_receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "3"))
        .await;

    session.when_disconnect_is_requested().await;
    mock_counterparty.then_gets_disconnected().await;
}

/// A new order message with an extra, invalid field.
#[derive(Clone, Debug)]
struct ExecutionReportWithInvalidField {
    order_id: String,
    exec_id: String,
    exec_type: fix44::ExecType,
    ord_status: fix44::OrdStatus,
    side: fix44::Side,
    symbol: String,
    order_qty: f64,
    price: f64,
    custom_field: String, // this field isn't recognised by our session
}

impl Default for ExecutionReportWithInvalidField {
    fn default() -> Self {
        Self {
            order_id: "ORD123".to_string(),
            exec_id: "EX123".to_string(),
            exec_type: fix44::ExecType::New,
            ord_status: fix44::OrdStatus::New,
            side: fix44::Side::Buy,
            symbol: "".to_string(),
            order_qty: 100.0,
            price: 100.0,
            custom_field: "Hello world".to_string(),
        }
    }
}

impl FixMessage for ExecutionReportWithInvalidField {
    fn write(&self, msg: &mut Message) {
        msg.set(fix44::ORDER_ID, self.order_id.as_str());
        msg.set(fix44::EXEC_ID, self.exec_id.as_str());
        msg.set(fix44::EXEC_TYPE, self.exec_type);
        msg.set(fix44::ORD_STATUS, self.ord_status);
        msg.set(fix44::SIDE, self.side);
        msg.set(fix44::SYMBOL, self.symbol.as_str());
        msg.set(fix44::ORDER_QTY, self.order_qty);
        msg.set(fix44::PRICE, self.price);

        // this is the important bit, we use a custom tag
        msg.set(CUSTOM_FIELD, self.custom_field.as_str());
    }

    fn message_type(&self) -> &str {
        "D"
    }

    fn parse(_message: &Message) -> Self {
        // we never parse this message
        unimplemented!()
    }
}

pub const CUSTOM_FIELD: &HardCodedFixFieldDefinition = &HardCodedFixFieldDefinition {
    name: "Custom Field",
    tag: 9999,
    data_type: FixDatatype::String,
    location: FieldLocation::Body,
};
