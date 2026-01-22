mod application;
mod messages;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use hotfix::config::SessionConfig;
use hotfix::field_types::{Date, Timestamp};
use hotfix::fix44;
use hotfix::fix44::OrdType;
use hotfix::initiator::Initiator;
use hotfix::session::SessionHandle;
use std::time::Instant;
use tokio::runtime::Builder;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::application::LoadTestingApplication;
use crate::messages::{ExecutionReport, NewOrderSingle, OutboundMsg};

#[derive(ValueEnum, Clone, Debug)]
#[clap(rename_all = "lower")]
enum Database {
    Memory,
    File,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "1000")]
    message_count: u32,
    #[arg(short, long, default_value = "2")]
    worker_threads: usize,
    #[arg(short, long)]
    database: Option<Database>,
}

const WAIT_SECONDS: u64 = 3;

fn main() -> Result<()> {
    let args = Args::parse();

    let runtime = Builder::new_multi_thread()
        .worker_threads(args.worker_threads)
        .thread_name("hotfix-worker")
        .enable_all()
        .build()
        .expect("runtime creation to succeed");

    runtime.block_on(run_load_test(
        args.message_count,
        args.database.unwrap_or(Database::Memory),
    ))?;

    Ok(())
}

async fn run_load_test(message_count: u32, database: Database) -> Result<()> {
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = get_config();

    let (tx, rx) = unbounded_channel();
    let application = LoadTestingApplication::new(tx);
    let initiator = start_session(config, database, application).await?;

    for s in 0..WAIT_SECONDS {
        info!("starting in {} seconds", WAIT_SECONDS - s);
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    let start = Instant::now();
    let messages_handler = tokio::spawn(submit_messages(initiator.session_handle(), message_count));
    let report_handler = tokio::spawn(listen_for_reports(rx, message_count));

    messages_handler.await?;
    info!("sent all messages, awaiting responses");
    report_handler.await?;

    let duration = start.elapsed();
    info!("completed run in {duration:?} seconds");

    initiator.shutdown(false).await?;

    Ok(())
}

async fn start_session(
    session_config: SessionConfig,
    db_config: Database,
    app: LoadTestingApplication,
) -> Result<Initiator<OutboundMsg>> {
    match db_config {
        Database::Memory => {
            let store = hotfix::store::in_memory::InMemoryMessageStore::default();
            Initiator::start(session_config, app, store).await
        }
        Database::File => {
            let store = hotfix::store::file::FileStore::new("data", "load-testing-store")
                .expect("be able to create store");
            Initiator::start(session_config, app, store).await
        }
    }
}

async fn submit_messages(session_handle: SessionHandle<OutboundMsg>, message_count: u32) {
    for _ in 0..message_count {
        submit_message(&session_handle).await;
    }
}

async fn submit_message(session_handle: &SessionHandle<OutboundMsg>) {
    let mut order_id = format!("{}", uuid::Uuid::new_v4());
    order_id.truncate(12);
    let order = OutboundMsg::NewOrderSingle(NewOrderSingle {
        transact_time: Timestamp::utc_now(),
        symbol: "EUR/USD".to_string(),
        cl_ord_id: order_id,
        side: fix44::Side::Buy,
        order_qty: 230,
        order_type: OrdType::Market,
        settlement_date: Date::new(2023, 9, 19).unwrap(),
        currency: "USD".to_string(),
        number_of_allocations: 1,
        allocation_account: "acc1".to_string(),
        allocation_quantity: 230,
    });

    session_handle
        .send_message(order)
        .await
        .expect("session to accept message");
}

async fn listen_for_reports(mut rx: UnboundedReceiver<ExecutionReport>, message_count: u32) {
    let mut count = 0u32;
    while let Some(_report) = rx.recv().await {
        count += 1;

        if count == message_count {
            break;
        }
    }

    info!("received {} reports", count);
}

fn get_config() -> SessionConfig {
    SessionConfig {
        begin_string: "FIX.4.4".to_string(),
        sender_comp_id: "dummy-initiator".to_string(),
        target_comp_id: "dummy-acceptor".to_string(),
        data_dictionary_path: None,
        connection_host: "127.0.0.1".to_string(),
        connection_port: 9880,
        tls_config: None,
        heartbeat_interval: 30,
        logon_timeout: 10,
        logout_timeout: 2,
        reconnect_interval: 30,
        reset_on_logon: true,
        schedule: None,
    }
}
