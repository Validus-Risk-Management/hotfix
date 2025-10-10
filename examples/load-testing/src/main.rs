mod application;
mod messages;

use crate::application::LoadTestingApplication;
use crate::messages::{ExecutionReport, Message, NewOrderSingle};
use clap::Parser;
use hotfix::config::SessionConfig;
use hotfix::field_types::{Date, Timestamp};
use hotfix::initiator::Initiator;
use hotfix::message::fix44;
use hotfix::message::fix44::OrdType;
use hotfix::session::SessionRef;
use std::time::Instant;
use tokio::runtime::Builder;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "10000")]
    message_count: u32,
    #[arg(short, long, default_value = "2")]
    worker_threads: usize,
}

const WAIT_SECONDS: u64 = 3;

fn main() {
    let args = Args::parse();

    let runtime = Builder::new_multi_thread()
        .worker_threads(args.worker_threads)
        .thread_name("hotfix-worker")
        .enable_all()
        .build()
        .expect("runtime creation to succeed");

    runtime.block_on(run_load_test(args.message_count));
}

async fn run_load_test(message_count: u32) {
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = get_config();
    // let store = hotfix::store::redb::RedbMessageStore::new("perf-session.db")
    //.expect("be able to create store");
    let store = hotfix::store::in_memory::InMemoryMessageStore::default();

    let (tx, rx) = unbounded_channel();
    let application = LoadTestingApplication::new(tx);

    let initiator = Initiator::start(config, application, store).await;

    for s in 0..WAIT_SECONDS {
        info!("starting in {} seconds", WAIT_SECONDS - s);
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    let start = Instant::now();
    let messages_handler = tokio::spawn(submit_messages(initiator.session_ref(), message_count));
    let report_handler = tokio::spawn(listen_for_reports(rx, message_count));

    messages_handler.await.unwrap();
    info!("sent all messages, awaiting responses");
    report_handler.await.unwrap();

    let duration = start.elapsed();
    info!("completed run in {duration:?} seconds");

    initiator
        .shutdown()
        .await
        .expect("graceful shutdown to succeed");
}

async fn submit_messages(session_ref: SessionRef<Message>, message_count: u32) {
    for _ in 0..message_count {
        submit_message(&session_ref).await;
    }
}

async fn submit_message(session_ref: &SessionRef<Message>) {
    let mut order_id = format!("{}", uuid::Uuid::new_v4());
    order_id.truncate(12);
    let order = NewOrderSingle {
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
    };
    let msg = Message::NewOrderSingle(order);

    session_ref.send_message(msg).await
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
        logon_timeout: 30,
        reconnect_interval: 30,
        reset_on_logon: true,
        schedule: None,
    }
}
