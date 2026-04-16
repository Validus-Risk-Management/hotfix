use hotfix::Message as HotfixMessage;
use hotfix::field_types::Timestamp;
use hotfix::fix44;
use hotfix::message::{OutboundMessage, Part};

use crate::custom_fix;

#[derive(Debug, Clone)]
pub struct NewOrderSingle {
    pub cl_ord_id: String,
    pub symbol: String,
    pub side: fix44::Side,
    pub order_qty: u32,
    pub transact_time: Timestamp,
    pub client_strategy_id: i32,
}

#[derive(Debug, Clone)]
pub enum OutboundMsg {
    NewOrderSingle(NewOrderSingle),
}

impl OutboundMessage for OutboundMsg {
    fn write(&self, msg: &mut HotfixMessage) {
        match self {
            OutboundMsg::NewOrderSingle(order) => {
                msg.set(fix44::CL_ORD_ID, order.cl_ord_id.as_str());
                msg.set(fix44::SYMBOL, order.symbol.as_str());
                msg.set(fix44::SIDE, order.side);
                msg.set(fix44::ORDER_QTY, order.order_qty);
                msg.set(fix44::TRANSACT_TIME, order.transact_time.clone());
                msg.set(fix44::ORD_TYPE, fix44::OrdType::Market);
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
