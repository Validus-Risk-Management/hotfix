mod application;
mod messages;

use clap::Parser;
use hotfix::config::Config;
use hotfix::field_types::{Date, Timestamp};
use hotfix::fix44;
use hotfix::initiator::Initiator;
use std::path::Path;
use tokio::task::spawn_blocking;
use tracing_subscriber::EnvFilter;

use crate::application::TestApplication;
use crate::messages::{Message, NewOrderSingle};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: String,
    #[arg(short, long)]
    logfile: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if let Some(path) = args.logfile {
        let p = Path::new(&path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let logfile = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(p)
            .expect("log file to open successfully");
        let subscriber = tracing_subscriber::fmt::Subscriber::builder()
            .with_writer(logfile)
            .with_env_filter(EnvFilter::from_default_env())
            .finish();
        tracing::subscriber::set_global_default(subscriber).unwrap();
    } else {
        tracing_subscriber::fmt()
            .pretty()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    }

    let app = TestApplication::default();
    let session = start_session(&args.config, app).await;

    user_loop(session).await;
}

async fn user_loop(session: Initiator<Message>) {
    loop {
        println!("(q) to quit, (s) to send message");

        let command_task = spawn_blocking(|| {
            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .expect("read line to succeed");
            input
        });

        match command_task.await.unwrap().trim() {
            "q" => {
                return;
            }
            "s" => {
                send_message(&session).await;
            }
            _ => {
                println!("Unrecognised command");
            }
        }
    }
}

async fn send_message(session: &Initiator<Message>) {
    let mut order_id = format!("{}", uuid::Uuid::new_v4());
    order_id.truncate(12);
    let order = NewOrderSingle {
        transact_time: Timestamp::utc_now(),
        symbol: "EUR/USD".to_string(),
        cl_ord_id: order_id,
        side: fix44::Side::Buy,
        order_qty: 230,
        settlement_date: Date::new(2023, 9, 19).unwrap(),
        currency: "USD".to_string(),
        number_of_allocations: 1,
        allocation_account: "acc1".to_string(),
        allocation_quantity: 230,
    };
    let msg = Message::NewOrderSingle(order);

    session.send_message(msg).await;
}

async fn start_session(config_path: &str, app: TestApplication) -> Initiator<Message> {
    let mut config = Config::load_from_path(config_path);
    let session_config = config.sessions.pop().expect("config to include a session");
    let store = hotfix::store::redb::RedbMessageStore::new("session.db");

    Initiator::new(session_config, app, store).await
}
