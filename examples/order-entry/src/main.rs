mod application;
mod messages;
mod web;

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use hotfix::config::Config;
use hotfix::initiator::Initiator;
use std::path::Path;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::application::{SharedState, TestApplication};
use crate::messages::OutboundMsg;
use crate::web::{OrderAppState, start_web_service};

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
    let shared_state = Arc::new(Mutex::new(SharedState::new()));
    let app = TestApplication::new(shared_state.clone());
    let initiator = start_session(&args.config, &db_config, app).await?;

    let status_service_token = CancellationToken::new();
    let session_handle = initiator.session_handle();
    let child_token = status_service_token.child_token();
    let order_app_state = OrderAppState {
        shared_state,
        initiator: initiator.clone(),
    };
    tokio::spawn(async move {
        if let Err(e) = start_web_service(session_handle, child_token, order_app_state).await {
            tracing::error!("web service error: {e:?}");
        }
    });

    tokio::signal::ctrl_c()
        .await
        .context("failed to listen for ctrl-c")?;
    info!("shutting down");

    status_service_token.cancel();
    initiator
        .shutdown(false)
        .await
        .context("graceful shutdown failed")?;
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

    let initiator = match db_config {
        Database::Memory => {
            let store = hotfix::store::in_memory::InMemoryMessageStore::default();
            Initiator::start(session_config, app, store).await?
        }
        Database::File => {
            let store = hotfix::store::file::FileStore::new("data", "order-entry-store")
                .context("failed to create file store")?;
            Initiator::start(session_config, app, store).await?
        }
    };

    Ok(initiator)
}
