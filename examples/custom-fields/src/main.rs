mod application;
mod custom_fix;
mod messages;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use hotfix::config::Config;
use hotfix::field_types::Timestamp;
use hotfix::initiator::Initiator;
use hotfix::store::in_memory::InMemoryMessageStore;
use tokio::sync::{Notify, mpsc};
use tokio::time::timeout;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use crate::application::TestApplication;
use crate::messages::{ExecReportSummary, NewOrderSingle, OutboundMsg};

const CONFIG_PATH: &str = "examples/custom-fields/config/test-config.toml";
const LOGON_TIMEOUT: Duration = Duration::from_secs(10);
const FILL_TIMEOUT: Duration = Duration::from_secs(10);
const STRATEGY_ID: i32 = 42;
const CL_ORD_ID: &str = "demo-1";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let mut config = Config::load_from_path(CONFIG_PATH).context("failed to load config")?;
    let session_config = config
        .sessions
        .pop()
        .context("config must include a session")?;

    let logon_signal = Arc::new(Notify::new());
    let (exec_tx, mut exec_rx) = mpsc::unbounded_channel::<ExecReportSummary>();

    let app = TestApplication {
        logon_signal: logon_signal.clone(),
        exec_tx,
    };

    let initiator: Initiator<OutboundMsg> =
        Initiator::start(session_config, app, InMemoryMessageStore::default())
            .await
            .context("failed to start initiator")?;

    info!("waiting for logon (up to {:?})", LOGON_TIMEOUT);
    timeout(LOGON_TIMEOUT, logon_signal.notified())
        .await
        .map_err(|_| anyhow!("session did not log on within {LOGON_TIMEOUT:?}"))?;

    let order = NewOrderSingle {
        cl_ord_id: CL_ORD_ID.to_string(),
        symbol: "EUR/USD".to_string(),
        side: custom_fix::Side::Buy,
        order_qty: 100,
        transact_time: Timestamp::utc_now(),
        client_strategy_id: STRATEGY_ID,
    };
    info!("sending NewOrderSingle ClOrdID={CL_ORD_ID} ClientStrategyId={STRATEGY_ID}");
    initiator
        .send(OutboundMsg::NewOrderSingle(order))
        .await
        .context("failed to send NewOrderSingle")?;

    let result = wait_for_fill(&mut exec_rx).await;

    info!("shutting down");
    if let Err(err) = initiator.shutdown(false).await {
        error!("graceful shutdown failed: {err}");
    }

    result
}

async fn wait_for_fill(exec_rx: &mut mpsc::UnboundedReceiver<ExecReportSummary>) -> Result<()> {
    let deadline = tokio::time::Instant::now() + FILL_TIMEOUT;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(anyhow!(
                "did not receive a Filled ExecutionReport within {FILL_TIMEOUT:?}"
            ));
        }

        let summary = match timeout(remaining, exec_rx.recv()).await {
            Ok(Some(s)) => s,
            Ok(None) => return Err(anyhow!("execution-report channel closed unexpectedly")),
            Err(_) => {
                return Err(anyhow!(
                    "did not receive a Filled ExecutionReport within {FILL_TIMEOUT:?}"
                ));
            }
        };

        info!(
            "received ExecutionReport ClOrdID={} OrdStatus={:?} ClientStrategyId={:?}",
            summary.cl_ord_id, summary.ord_status, summary.client_strategy_id,
        );

        let echoed = summary.client_strategy_id.ok_or_else(|| {
            anyhow!(
                "ExecutionReport for ClOrdID={} did not echo ClientStrategyId — \
                 the acceptor likely doesn't know about tag 6001",
                summary.cl_ord_id,
            )
        })?;

        if echoed != STRATEGY_ID {
            return Err(anyhow!(
                "ExecutionReport ClientStrategyId mismatch: expected {STRATEGY_ID}, got {echoed}",
            ));
        }

        if matches!(summary.ord_status, custom_fix::OrdStatus::Filled) {
            info!("order filled, custom field round-tripped successfully");
            return Ok(());
        }
    }
}
