use clap::Parser;
use hotfix_cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    hotfix_cli::run(cli).await
}
