use hotfix::Message as HotfixMessage;
use hotfix::message::FixMessage;
use hotfix_message::{Part, fix44};

/// Business messages used for testing.
#[derive(Debug, Clone)]
pub enum TestMessage {
    /// A minimal implementation of a valid execution report.
    MinimalExecutionReport {
        order_id: String,
        exec_id: String,
        exec_type: fix44::ExecType,
        ord_status: fix44::OrdStatus,
        side: fix44::Side,
        symbol: String,
        order_qty: f64,
        price: f64,
    },
}

impl TestMessage {
    pub fn dummy_execution_report() -> Self {
        Self::MinimalExecutionReport {
            order_id: "123456789".to_string(),
            exec_id: "123456789".to_string(),
            exec_type: fix44::ExecType::New,
            ord_status: fix44::OrdStatus::New,
            side: fix44::Side::Buy,
            symbol: "ABC".to_string(),
            order_qty: 100.0,
            price: 100.0,
        }
    }
}

impl FixMessage for TestMessage {
    fn write(&self, msg: &mut HotfixMessage) {
        match self {
            TestMessage::MinimalExecutionReport {
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
        }
    }

    fn message_type(&self) -> &str {
        match self {
            TestMessage::MinimalExecutionReport { .. } => "8",
        }
    }

    fn parse(msg: &HotfixMessage) -> Self {
        let msg_type: &str = msg.get(fix44::MSG_TYPE).unwrap();
        if msg_type != "8" {
            // not an execution report
            panic!("Invalid message type: {msg_type}");
        }

        let order_id: &str = msg.get(fix44::ORDER_ID).unwrap();
        let exec_id: &str = msg.get(fix44::EXEC_ID).unwrap();
        let exec_type = msg.get(fix44::EXEC_TYPE).unwrap();
        let ord_status = msg.get(fix44::ORD_STATUS).unwrap();
        let side = msg.get(fix44::SIDE).unwrap();
        let symbol: &str = msg.get(fix44::SYMBOL).unwrap();
        let order_qty = msg.get(fix44::ORDER_QTY).unwrap();
        let price = msg.get(fix44::PRICE).unwrap();

        Self::MinimalExecutionReport {
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
}
