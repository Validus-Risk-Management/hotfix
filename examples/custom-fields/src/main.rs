mod custom_fix;
mod messages;

use hotfix::field_types::Timestamp;
use hotfix::fix44;

use crate::messages::{NewOrderSingle, OutboundMsg};

fn main() {
    let _order = OutboundMsg::NewOrderSingle(NewOrderSingle {
        cl_ord_id: "demo-1".to_string(),
        symbol: "EUR/USD".to_string(),
        side: fix44::Side::Buy,
        order_qty: 100,
        transact_time: Timestamp::utc_now(),
        client_strategy_id: 42,
    });

    println!(
        "constructed NewOrderSingle (custom tag {} = 42)",
        custom_fix::CLIENT_STRATEGY_ID.tag,
    );
}
