use hotfix::Message as HotfixMessage;
use hotfix::field_types::{Date, Timestamp};
use hotfix::fix44;
use hotfix::fix44::{OrdStatus, OrdType, Side};
use hotfix::message::{OutboundMessage, Part, RepeatingGroup};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ExecutionReport {
    order_id: String,
    cl_ord_id: String,
    exec_id: String,
    exec_type: String,
    ord_status: OrdStatus,
    side: Side,
    symbol: String,
    leaves_qty: f64,
    cum_qty: f64,
    avx_px: f64,
}

#[derive(Debug, Clone)]
pub struct NewOrderSingle {
    // order details
    pub transact_time: Timestamp,
    pub symbol: String,    // CCY1/CCY2 as string
    pub cl_ord_id: String, // unique order ID assigned by the customer
    pub side: Side,
    pub order_qty: u32,
    pub order_type: OrdType,
    pub settlement_date: Date,
    pub currency: String, // the dealt currency

    // allocation
    pub number_of_allocations: u32,
    pub allocation_account: String,
    pub allocation_quantity: u32,
}

#[derive(Debug, Clone)]
pub enum InboundMsg {
    ExecutionReport(ExecutionReport),
    Unimplemented(Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum OutboundMsg {
    NewOrderSingle(NewOrderSingle),
}

impl OutboundMessage for OutboundMsg {
    fn write(&self, msg: &mut HotfixMessage) {
        match self {
            OutboundMsg::NewOrderSingle(order) => {
                // order details
                msg.set(fix44::TRANSACT_TIME, order.transact_time.clone());
                msg.set(fix44::SYMBOL, order.symbol.as_str());
                msg.set(fix44::CL_ORD_ID, order.cl_ord_id.as_str());
                msg.set(fix44::SIDE, order.side);
                msg.set(fix44::ORDER_QTY, order.order_qty);
                msg.set(fix44::ORD_TYPE, order.order_type);
                msg.set(fix44::SETTL_DATE, order.settlement_date);
                msg.set(fix44::CURRENCY, order.currency.as_str());

                // allocations
                msg.set(fix44::NO_ALLOCS, order.number_of_allocations);
                let mut allocation = RepeatingGroup::new(fix44::NO_ALLOCS, fix44::ALLOC_ACCOUNT);
                allocation.set(fix44::ALLOC_ACCOUNT, order.allocation_account.as_str());
                allocation.set(fix44::ALLOC_QTY, order.allocation_quantity);
                msg.set_groups(vec![allocation]).unwrap();
            }
        }
    }

    fn message_type(&self) -> &str {
        match self {
            OutboundMsg::NewOrderSingle(_) => "D",
        }
    }
}

impl InboundMsg {
    pub fn parse(message: &HotfixMessage) -> Self {
        let message_type: &str = message.header().get(fix44::MSG_TYPE).unwrap();
        if message_type == "8" {
            Self::parse_execution_report_ack(message)
        } else {
            Self::Unimplemented(message_type.as_bytes().to_vec())
        }
    }

    fn parse_execution_report_ack(message: &HotfixMessage) -> Self {
        let report = ExecutionReport {
            order_id: message.get::<&str>(fix44::ORDER_ID).unwrap().to_string(),
            cl_ord_id: message.get::<&str>(fix44::CL_ORD_ID).unwrap().to_string(),
            exec_id: message.get::<&str>(fix44::EXEC_ID).unwrap().to_string(),
            exec_type: message.get::<&str>(fix44::EXEC_TYPE).unwrap().to_string(),
            ord_status: message.get::<OrdStatus>(fix44::ORD_STATUS).unwrap(),
            side: message.get::<Side>(fix44::SIDE).unwrap(),
            symbol: message.get::<&str>(fix44::SYMBOL).unwrap().to_string(),
            leaves_qty: message.get::<f64>(fix44::LEAVES_QTY).unwrap(),
            cum_qty: message.get::<f64>(fix44::CUM_QTY).unwrap(),
            avx_px: message.get::<f64>(fix44::AVG_PX).unwrap(),
        };
        Self::ExecutionReport(report)
    }
}
