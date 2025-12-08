use crate::common::setup::{COUNTERPARTY_COMP_ID, OUR_COMP_ID};
use chrono::TimeDelta;
use hotfix::Message as HotfixMessage;
use hotfix::message::{FixMessage, generate_message};
use hotfix_message::dict::{FieldLocation, FixDatatype};
use hotfix_message::field_types::Timestamp;
use hotfix_message::message::{Config, Message};
use hotfix_message::{HardCodedFixFieldDefinition, Part, fix44};
use std::ops::Add;

/// Business messages used for testing.
#[derive(Debug, Clone)]
pub enum TestMessage {
    /// A minimal implementation of a valid execution report.
    ExecutionReport {
        order_id: String,
        exec_id: String,
        exec_type: fix44::ExecType,
        ord_status: fix44::OrdStatus,
        side: fix44::Side,
        symbol: String,
        order_qty: f64,
        price: f64,
    },
    /// A minimal implementation of a valid new order single.
    NewOrderSingle {
        cl_ord_id: String,
        side: fix44::Side,
        symbol: String,
        order_qty: f64,
        ord_type: fix44::OrdType,
        price: f64,
    },
}

impl TestMessage {
    pub fn dummy_execution_report() -> Self {
        Self::dummy_execution_report_with_order_id("123456789".to_string())
    }

    pub fn dummy_execution_report_with_order_id(order_id: String) -> Self {
        Self::ExecutionReport {
            order_id,
            exec_id: "123456789".to_string(),
            exec_type: fix44::ExecType::New,
            ord_status: fix44::OrdStatus::New,
            side: fix44::Side::Buy,
            symbol: "EUR/USD".to_string(),
            order_qty: 100.0,
            price: 100.0,
        }
    }

    pub fn dummy_new_order_single() -> Self {
        Self::NewOrderSingle {
            cl_ord_id: "123456789".to_string(),
            side: fix44::Side::Buy,
            symbol: "EUR/USD".to_string(),
            order_qty: 100.0,
            ord_type: fix44::OrdType::Limit,
            price: 100.0,
        }
    }
}

impl FixMessage for TestMessage {
    fn write(&self, msg: &mut HotfixMessage) {
        match self {
            TestMessage::ExecutionReport {
                order_id,
                exec_id,
                exec_type,
                ord_status,
                side,
                symbol,
                order_qty,
                price,
            } => {
                msg.set(fix44::ORDER_ID, order_id.as_str());
                msg.set(fix44::EXEC_ID, exec_id.as_str());
                msg.set(fix44::EXEC_TYPE, *exec_type);
                msg.set(fix44::ORD_STATUS, *ord_status);
                msg.set(fix44::SIDE, *side);
                msg.set(fix44::SYMBOL, symbol.as_str());
                msg.set(fix44::ORDER_QTY, *order_qty);
                msg.set(fix44::PRICE, *price);
            }
            TestMessage::NewOrderSingle {
                cl_ord_id,
                side,
                symbol,
                order_qty,
                ord_type,
                price,
            } => {
                msg.set(fix44::CL_ORD_ID, cl_ord_id.as_str());
                msg.set(fix44::SIDE, *side);
                msg.set(fix44::SYMBOL, symbol.as_str());
                msg.set(fix44::ORDER_QTY, *order_qty);
                msg.set(fix44::ORD_TYPE, *ord_type);
                msg.set(fix44::PRICE, *price);
            }
        }
    }

    fn message_type(&self) -> &str {
        match self {
            TestMessage::ExecutionReport { .. } => "8",
            TestMessage::NewOrderSingle { .. } => "D",
        }
    }

    fn parse(msg: &HotfixMessage) -> Self {
        let msg_type: &str = msg.header().get(fix44::MSG_TYPE).unwrap();
        match msg_type {
            "8" => {
                // Execution Report
                let order_id: &str = msg.get(fix44::ORDER_ID).unwrap();
                let exec_id: &str = msg.get(fix44::EXEC_ID).unwrap();
                let exec_type = msg.get(fix44::EXEC_TYPE).unwrap();
                let ord_status = msg.get(fix44::ORD_STATUS).unwrap();
                let side = msg.get(fix44::SIDE).unwrap();
                let symbol: &str = msg.get(fix44::SYMBOL).unwrap();
                let order_qty = msg.get(fix44::ORDER_QTY).unwrap();
                let price = msg.get(fix44::PRICE).unwrap();

                Self::ExecutionReport {
                    order_id: order_id.to_string(),
                    exec_id: exec_id.to_string(),
                    exec_type,
                    ord_status,
                    side,
                    symbol: symbol.to_string(),
                    order_qty,
                    price,
                }
            }
            "D" => {
                // New Order Single
                let cl_ord_id: &str = msg.get(fix44::CL_ORD_ID).unwrap();
                let side = msg.get(fix44::SIDE).unwrap();
                let symbol: &str = msg.get(fix44::SYMBOL).unwrap();
                let order_qty = msg.get(fix44::ORDER_QTY).unwrap();
                let ord_type = msg.get(fix44::ORD_TYPE).unwrap();
                let price = msg.get(fix44::PRICE).unwrap();

                Self::NewOrderSingle {
                    cl_ord_id: cl_ord_id.to_string(),
                    side,
                    symbol: symbol.to_string(),
                    order_qty,
                    ord_type,
                    price,
                }
            }
            _ => panic!("Invalid message type: {msg_type}"),
        }
    }
}

/// A new order message with an extra, invalid field.
#[derive(Clone, Debug)]
pub struct ExecutionReportWithInvalidField {
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

pub fn build_execution_report_with_incorrect_body_length(msg_seq_num: u64) -> Vec<u8> {
    let report = TestMessage::dummy_execution_report();
    let mut raw_message = generate_message(
        "FIX.4.4",
        COUNTERPARTY_COMP_ID,
        OUR_COMP_ID,
        msg_seq_num,
        report,
    )
    .unwrap();

    replace_field_value(&mut raw_message, 9, b"999");

    raw_message
}

pub fn build_execution_report_with_incorrect_begin_string(msg_seq_num: u64) -> Vec<u8> {
    let report = TestMessage::dummy_execution_report();

    // we expect BeginString FIX.4.4 but this message contains FIX.4.2
    let mut msg = Message::new("FIX.4.2", report.message_type());
    msg.set(fix44::SENDER_COMP_ID, COUNTERPARTY_COMP_ID);
    msg.set(fix44::TARGET_COMP_ID, OUR_COMP_ID);
    msg.set(fix44::MSG_SEQ_NUM, msg_seq_num);
    msg.set(fix44::SENDING_TIME, Timestamp::utc_now());

    report.write(&mut msg);

    msg.encode(&Config::default()).unwrap()
}

pub fn build_execution_report_with_comp_id(
    msg_seq_num: u64,
    sender_comp_id: &str,
    target_comp_id: &str,
) -> Vec<u8> {
    let report = TestMessage::dummy_execution_report();

    let mut msg = Message::new("FIX.4.4", report.message_type());
    msg.set(fix44::SENDER_COMP_ID, sender_comp_id);
    msg.set(fix44::TARGET_COMP_ID, target_comp_id);
    msg.set(fix44::MSG_SEQ_NUM, msg_seq_num);
    msg.set(fix44::SENDING_TIME, Timestamp::utc_now());

    report.write(&mut msg);

    msg.encode(&Config::default()).unwrap()
}

pub fn build_execution_report_with_custom_msg_type(msg_seq_num: u64, msg_type: &str) -> Vec<u8> {
    let report = TestMessage::dummy_execution_report();

    let mut msg = Message::new("FIX.4.4", msg_type);
    msg.set(fix44::SENDER_COMP_ID, COUNTERPARTY_COMP_ID);
    msg.set(fix44::TARGET_COMP_ID, OUR_COMP_ID);
    msg.set(fix44::MSG_SEQ_NUM, msg_seq_num);
    msg.set(fix44::SENDING_TIME, Timestamp::utc_now());

    report.write(&mut msg);

    msg.encode(&Config::default()).unwrap()
}

pub fn build_execution_report_with_incorrect_orig_sending_time(msg_seq_num: u64) -> Vec<u8> {
    let report = TestMessage::dummy_execution_report();

    let mut msg = Message::new("FIX.4.4", "8");
    msg.set(fix44::SENDER_COMP_ID, COUNTERPARTY_COMP_ID);
    msg.set(fix44::TARGET_COMP_ID, OUR_COMP_ID);
    msg.set(fix44::MSG_SEQ_NUM, msg_seq_num);

    let sending_time = Timestamp::utc_now();
    let original_sending_time: Timestamp = sending_time
        .to_chrono_naive()
        .unwrap()
        .add(TimeDelta::seconds(1))
        .into();
    msg.set(fix44::SENDING_TIME, sending_time);
    msg.set(fix44::ORIG_SENDING_TIME, original_sending_time);
    msg.set(fix44::POSS_DUP_FLAG, "Y");

    report.write(&mut msg);

    msg.encode(&Config::default()).unwrap()
}

pub fn build_execution_report_with_missing_orig_sending_time(msg_seq_num: u64) -> Vec<u8> {
    let report = TestMessage::dummy_execution_report();

    let mut msg = Message::new("FIX.4.4", "8");
    msg.set(fix44::SENDER_COMP_ID, COUNTERPARTY_COMP_ID);
    msg.set(fix44::TARGET_COMP_ID, OUR_COMP_ID);
    msg.set(fix44::MSG_SEQ_NUM, msg_seq_num);
    msg.set(fix44::SENDING_TIME, Timestamp::utc_now());
    msg.set(fix44::POSS_DUP_FLAG, "Y");

    report.write(&mut msg);

    msg.encode(&Config::default()).unwrap()
}

pub fn build_execution_report_with_missing_sending_time(msg_seq_num: u64) -> Vec<u8> {
    let report = TestMessage::dummy_execution_report();

    let mut msg = Message::new("FIX.4.4", "8");
    msg.set(fix44::SENDER_COMP_ID, COUNTERPARTY_COMP_ID);
    msg.set(fix44::TARGET_COMP_ID, OUR_COMP_ID);
    msg.set(fix44::MSG_SEQ_NUM, msg_seq_num);
    // Don't set SENDING_TIME

    report.write(&mut msg);

    msg.encode(&Config::default()).unwrap()
}

pub fn build_execution_report_with_sending_time_too_old(msg_seq_num: u64) -> Vec<u8> {
    let report = TestMessage::dummy_execution_report();

    let mut msg = Message::new("FIX.4.4", "8");
    msg.set(fix44::SENDER_COMP_ID, COUNTERPARTY_COMP_ID);
    msg.set(fix44::TARGET_COMP_ID, OUR_COMP_ID);
    msg.set(fix44::MSG_SEQ_NUM, msg_seq_num);

    // Set sending time to 121 seconds in the past (beyond the 120 second threshold)
    let now = chrono::Utc::now();
    let past_time = now - TimeDelta::seconds(121);
    let past_timestamp: Timestamp = past_time.naive_utc().into();
    msg.set(fix44::SENDING_TIME, past_timestamp);

    report.write(&mut msg);

    msg.encode(&Config::default()).unwrap()
}

/// Replaces the value of a field in a raw FIX message.
pub fn replace_field_value(raw_message: &mut Vec<u8>, tag: u32, new_value: &[u8]) {
    let tag_bytes = format!("{}=", tag).into_bytes();

    if let Some(field_start) = raw_message
        .windows(tag_bytes.len())
        .position(|window| window == tag_bytes)
    {
        let value_start = field_start + tag_bytes.len();
        if let Some(field_end) = raw_message[value_start..]
            .iter()
            .position(|&b| b == b'\x01')
        {
            let value_end = value_start + field_end;

            raw_message.splice(value_start..value_end, new_value.iter().cloned());
        }
    }
}

/// Builds a resend request message without the required BeginSeqNo field.
pub fn build_invalid_resend_request(
    msg_seq_num: u64,
    begin_seq_no: Option<u64>,
    end_seq_no: Option<u64>,
) -> Vec<u8> {
    let mut msg = Message::new("FIX.4.4", "2"); // MsgType 2 = ResendRequest
    msg.set(fix44::SENDER_COMP_ID, COUNTERPARTY_COMP_ID);
    msg.set(fix44::TARGET_COMP_ID, OUR_COMP_ID);
    msg.set(fix44::MSG_SEQ_NUM, msg_seq_num);
    msg.set(fix44::SENDING_TIME, Timestamp::utc_now());

    if let Some(begin_seq_no) = begin_seq_no {
        msg.set(fix44::BEGIN_SEQ_NO, begin_seq_no);
    }
    if let Some(end_seq_no) = end_seq_no {
        msg.set(fix44::END_SEQ_NO, end_seq_no);
    }

    msg.encode(&Config::default()).unwrap()
}

pub fn build_sequence_reset_without_new_seq_no(msg_seq_num: u64) -> Vec<u8> {
    let mut msg = Message::new("FIX.4.4", "4"); // MsgType 4 = SequenceReset
    msg.set(fix44::SENDER_COMP_ID, COUNTERPARTY_COMP_ID);
    msg.set(fix44::TARGET_COMP_ID, OUR_COMP_ID);
    msg.set(fix44::MSG_SEQ_NUM, msg_seq_num);
    msg.set(fix44::SENDING_TIME, Timestamp::utc_now());
    // Deliberately omit NEW_SEQ_NO to create an invalid SequenceReset

    msg.encode(&Config::default()).unwrap()
}
