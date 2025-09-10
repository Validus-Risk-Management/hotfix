use crate::common::actions::when;
use crate::common::assertions::then;
use crate::common::setup::given_an_active_session;
use crate::common::test_messages::{TestMessage, replace_field_value};
use hotfix::message::{FixMessage, generate_message};
use hotfix::session::Status;
use hotfix_message::dict::{FieldLocation, FixDatatype};
use hotfix_message::fix44::MSG_TYPE;
use hotfix_message::message::Message;
use hotfix_message::{HardCodedFixFieldDefinition, Part, fix44};

/// Tests that when a counterparty sends a message containing an invalid/unrecognised field,
/// the session rejects the message by sending a Reject (MsgType=3) message back.
#[tokio::test]
async fn test_message_with_invalid_field_gets_rejected() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    when(&mut mock_counterparty)
        .sends_message(ExecutionReportWithInvalidField::default())
        .await;
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "3"))
        .await;

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
}

/// Tests that when a counterparty sends a garbled message with an invalid body length,
/// the session silently ignores it and detects a sequence gap when the next valid message arrives.
#[tokio::test]
async fn test_garbled_message_with_invalid_target_comp_id_gets_ignored() {
    let (session, mut mock_counterparty) = given_an_active_session().await;

    // counterparty sends a message with invalid body length, which constitutes a garbled message
    let garbled_message = build_execution_report_with_incorrect_body_length(
        "dummy-acceptor",
        "dummy-initiator",
        mock_counterparty.next_target_sequence_number(),
    );
    when(&mut mock_counterparty)
        .sends_raw_message(garbled_message)
        .await;

    // they then send a valid message
    when(&mut mock_counterparty)
        .sends_message(TestMessage::dummy_execution_report())
        .await;

    // we then initiate a resend, having skipped the garbled message
    then(&mut mock_counterparty)
        .receives(|msg| assert_eq!(msg.header().get::<&str>(MSG_TYPE).unwrap(), "2"))
        .await;
    then(&session)
        .status_changes_to(Status::AwaitingResend)
        .await;

    when(&session).requests_disconnect().await;
    then(&mut mock_counterparty).gets_disconnected().await;
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

fn build_execution_report_with_incorrect_body_length(
    sender_comp_id: &str,
    target_comp_id: &str,
    msg_seq_num: usize,
) -> Vec<u8> {
    let report = TestMessage::dummy_execution_report();
    let mut raw_message =
        generate_message(sender_comp_id, target_comp_id, msg_seq_num, report).unwrap();

    replace_field_value(&mut raw_message, 9, b"999");

    raw_message
}
