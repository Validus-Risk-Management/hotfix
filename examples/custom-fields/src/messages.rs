use hotfix::Message as HotfixMessage;
use hotfix::field_types::Timestamp;
use hotfix::message::{OutboundMessage, Part};

use crate::custom_fix;

#[derive(Debug, Clone)]
pub struct NewOrderSingle {
    pub cl_ord_id: String,
    pub symbol: String,
    pub side: custom_fix::Side,
    pub order_qty: u32,
    pub transact_time: Timestamp,
    pub client_strategy_id: i32,
}

#[derive(Debug, Clone)]
pub enum OutboundMsg {
    NewOrderSingle(NewOrderSingle),
}

#[derive(Debug, Clone)]
pub struct ExecReportSummary {
    pub cl_ord_id: String,
    pub ord_status: custom_fix::OrdStatus,
    pub client_strategy_id: Option<i32>,
}

impl OutboundMessage for OutboundMsg {
    fn write(&self, msg: &mut HotfixMessage) {
        match self {
            OutboundMsg::NewOrderSingle(order) => {
                msg.set(custom_fix::CL_ORD_ID, order.cl_ord_id.as_str());
                msg.set(custom_fix::SYMBOL, order.symbol.as_str());
                msg.set(custom_fix::SIDE, order.side);
                msg.set(custom_fix::ORDER_QTY, order.order_qty);
                msg.set(custom_fix::TRANSACT_TIME, order.transact_time.clone());
                msg.set(custom_fix::ORD_TYPE, custom_fix::OrdType::Market);
                msg.set(custom_fix::CLIENT_STRATEGY_ID, order.client_strategy_id);
            }
        }
    }

    fn message_type(&self) -> &str {
        match self {
            OutboundMsg::NewOrderSingle(_) => "D",
        }
    }
}
