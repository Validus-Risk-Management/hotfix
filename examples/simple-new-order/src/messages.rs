use hotfix::field_types::{Date, Timestamp};
use hotfix::message::{fix44, FixMessage, Part, RepeatingGroup};
use hotfix::Message as HotfixMessage;

#[derive(Debug, Clone)]
pub struct NewOrderSingle {
    // order details
    pub transact_time: Timestamp,
    pub symbol: String,    // CCY1/CCY2 as string
    pub cl_ord_id: String, // unique order ID assigned by the customer
    pub side: fix44::Side,
    pub order_qty: u32,
    pub settlement_date: Date,
    pub currency: String, // the dealt currency

    // allocation
    pub number_of_allocations: u32,
    pub allocation_account: String,
    pub allocation_quantity: u32,
}

#[derive(Debug, Clone)]
pub enum Message {
    NewOrderSingle(NewOrderSingle),
    UnimplementedMessage(Vec<u8>),
}

impl FixMessage for Message {
    fn write(&self, msg: &mut HotfixMessage) {
        match self {
            Self::NewOrderSingle(order) => {
                // order details
                msg.set(fix44::TRANSACT_TIME, order.transact_time.clone());
                msg.set(fix44::SYMBOL, order.symbol.as_str());
                msg.set(fix44::CL_ORD_ID, order.cl_ord_id.as_str());
                msg.set(fix44::SIDE, order.side);
                msg.set(fix44::ORDER_QTY, order.order_qty);
                msg.set(fix44::SETTL_DATE, order.settlement_date);
                msg.set(fix44::CURRENCY, order.currency.as_str());

                // allocations
                msg.set(fix44::NO_ALLOCS, order.number_of_allocations);
                let mut allocation = RepeatingGroup::new(fix44::NO_ALLOCS, fix44::ALLOC_ACCOUNT);
                allocation.set(fix44::ALLOC_ACCOUNT, order.allocation_account.as_str());
                allocation.set(fix44::ALLOC_QTY, order.allocation_quantity);
                msg.set_groups(vec![allocation]);
            }
            _ => unimplemented!(),
        }
    }

    fn message_type(&self) -> &str {
        match self {
            Self::NewOrderSingle(_) => "D",
            _ => unimplemented!(),
        }
    }

    fn parse(message: &HotfixMessage) -> Self {
        let message_type: &str = message.header().get(fix44::MSG_TYPE).unwrap();
        Self::UnimplementedMessage(message_type.as_bytes().to_vec())
    }
}
