mod application;
mod messages;

use crate::application::TestApplication;
use crate::messages::{Message, NewOrderSingle};
use clap::{Parser, ValueEnum};
use hotfix::config::Config;
use hotfix::field_types::{Date, Timestamp};
use hotfix::initiator::Initiator;
use hotfix::message::fix44;
use hotfix::session::SessionHandle;
use hotfix::store::mongodb::Client;
use hotfix_web::{RouterConfig, build_router_with_config};
use std::path::Path;
use tokio::select;
use tokio::task::spawn_blocking;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(ValueEnum, Clone, Debug)]
#[clap(rename_all = "lower")]
enum Database {
    Redb,
    Mongodb,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: String,
    #[arg(short, long)]
    logfile: Option<String>,
    #[arg(short, long)]
    database: Option<Database>,
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
            .truncate(true)
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

    let db_config = args.database.unwrap_or(Database::Redb);
    let app = TestApplication::default();
    let initiator = start_session(&args.config, &db_config, app).await;

    let status_service_token = CancellationToken::new();
    tokio::spawn(start_web_service(
        initiator.session_handle(),
        status_service_token.child_token(),
    ));

    user_loop(&initiator).await;
    status_service_token.cancel();
    initiator
        .shutdown(false)
        .await
        .expect("graceful shutdown to succeed");
}

async fn user_loop(session: &Initiator<Message>) {
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
                send_message(session).await;
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

    session.send_message(msg).await.unwrap();
}

async fn start_session(
    config_path: &str,
    db_config: &Database,
    app: TestApplication,
) -> Initiator<Message> {
    let mut config = Config::load_from_path(config_path);
    let session_config = config.sessions.pop().expect("config to include a session");

    match db_config {
        Database::Redb => {
            let store = hotfix::store::redb::RedbMessageStore::new("session.db")
                .expect("be able to create store");
            Initiator::start(session_config, app, store).await
        }
        Database::Mongodb => {
            let uri = "mongodb://localhost:30001";
            let client = Client::with_uri_str(uri)
                .await
                .expect("able to create client");
            let store =
                hotfix::store::mongodb::MongoDbMessageStore::new(client.database("hotfix"), None)
                    .await
                    .expect("be able to create store");
            Initiator::start(session_config, app, store).await
        }
    }
}

async fn start_web_service(
    session_handle: SessionHandle<Message>,
    cancellation_token: CancellationToken,
) {
    let config = RouterConfig {
        enable_admin_endpoints: true,
    };
    let router = build_router_with_config(session_handle, config);
    let host_and_port = std::env::var("HOST_AND_PORT").unwrap_or("0.0.0.0:9881".to_string());
    let listener = tokio::net::TcpListener::bind(&host_and_port).await.unwrap();

    info!("starting web interface on http://{host_and_port}");

    select! {
        result = axum::serve(listener, router) => {
            if let Err(e) = result {
                tracing::error!("status service error: {}", e);
            }
        },
        () = cancellation_token.cancelled() => {
            info!("status service cancelled");
        }
    }
}
