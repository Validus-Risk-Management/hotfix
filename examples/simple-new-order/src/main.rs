mod application;
mod messages;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use hotfix::config::Config;
use hotfix::field_types::{Date, Timestamp};
use hotfix::fix44;
use hotfix::initiator::Initiator;
use hotfix::session::SessionHandle;
use hotfix_web::{RouterConfig, build_router_with_config};
use std::path::Path;
use tokio::select;
use tokio::task::spawn_blocking;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::application::TestApplication;
use crate::messages::{NewOrderSingle, OutboundMsg};

#[derive(ValueEnum, Clone, Debug)]
#[clap(rename_all = "lower")]
enum Database {
    Memory,
    File,
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
async fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(path) = args.logfile {
        let p = Path::new(&path);
        let parent = p
            .parent()
            .context("log file path has no parent directory")?;
        std::fs::create_dir_all(parent)?;
        let logfile = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(p)
            .context("failed to open log file")?;
        let subscriber = tracing_subscriber::fmt::Subscriber::builder()
            .with_writer(logfile)
            .with_env_filter(EnvFilter::from_default_env())
            .finish();
        tracing::subscriber::set_global_default(subscriber)?;
    } else {
        tracing_subscriber::fmt()
            .pretty()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    }

    let db_config = args.database.unwrap_or(Database::Memory);
    let app = TestApplication::default();
    let initiator = start_session(&args.config, &db_config, app).await?;

    let status_service_token = CancellationToken::new();
    let session_handle = initiator.session_handle();
    let child_token = status_service_token.child_token();
    tokio::spawn(async move {
        if let Err(e) = start_web_service(session_handle, child_token).await {
            tracing::error!("web service error: {e:?}");
        }
    });

    user_loop(&initiator).await?;
    status_service_token.cancel();
    initiator
        .shutdown(false)
        .await
        .context("graceful shutdown failed")?;
    Ok(())
}

async fn user_loop(session: &Initiator<OutboundMsg>) -> Result<()> {
    loop {
        println!("(q) to quit, (s) to send message");

        let command_task = spawn_blocking(|| -> Result<String> {
            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .context("failed to read line from stdin")?;
            Ok(input)
        });

        let input: String = command_task
            .await
            .context("failed to join blocking task")??;

        match input.trim() {
            "q" => {
                return Ok(());
            }
            "s" => {
                send_message(session).await?;
            }
            _ => {
                println!("Unrecognised command");
            }
        }
    }
}

async fn send_message(session: &Initiator<OutboundMsg>) -> Result<()> {
    let mut order_id = format!("{}", uuid::Uuid::new_v4());
    order_id.truncate(12);
    let order = NewOrderSingle {
        transact_time: Timestamp::utc_now(),
        symbol: "EUR/USD".to_string(),
        cl_ord_id: order_id,
        side: fix44::Side::Buy,
        order_qty: 230,
        settlement_date: Date::new(2023, 9, 19).context("invalid settlement date")?,
        currency: "USD".to_string(),
        number_of_allocations: 1,
        allocation_account: "acc1".to_string(),
        allocation_quantity: 230,
    };
    let msg = OutboundMsg::NewOrderSingle(order);

    session
        .send_message(msg)
        .await
        .context("failed to send message")?;
    Ok(())
}

async fn start_session(
    config_path: &str,
    db_config: &Database,
    app: TestApplication,
) -> Result<Initiator<OutboundMsg>> {
    let mut config = Config::load_from_path(config_path)?;
    let session_config = config
        .sessions
        .pop()
        .context("config must include a session")?;

    match db_config {
        Database::Memory => {
            let store = hotfix::store::in_memory::InMemoryMessageStore::default();
            Initiator::start(session_config, app, store).await
        }
        Database::File => {
            let store = hotfix::store::file::FileStore::new("data", "simple-new-order-store")
                .context("failed to create file store")?;
            Initiator::start(session_config, app, store).await
        }
    }
}

async fn start_web_service(
    session_handle: SessionHandle<OutboundMsg>,
    cancellation_token: CancellationToken,
) -> Result<()> {
    let config = RouterConfig {
        enable_admin_endpoints: true,
    };
    let router = build_router_with_config(session_handle, config);
    let host_and_port = std::env::var("HOST_AND_PORT").unwrap_or("0.0.0.0:9881".to_string());
    let listener = tokio::net::TcpListener::bind(&host_and_port)
        .await
        .context("failed to bind TCP listener")?;

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

    Ok(())
}
