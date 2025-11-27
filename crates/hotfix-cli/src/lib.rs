use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use serde::Deserialize;

#[derive(Parser)]
#[command(name = "hotfix")]
#[command(about = "CLI tool for managing hotfix sessions", long_about = None)]
pub struct Cli {
    /// Base URL of the hotfix web server
    #[arg(
        short,
        long,
        env = "HOTFIX_WEB_URL",
        default_value = "http://localhost:9881"
    )]
    pub url: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Check the health status of the server
    Health,
    /// Get current session information
    SessionInfo,
    /// Request a reset on next logon
    Reset,
    /// Shutdown the session
    Shutdown,
}

#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionInfoResponse {
    pub session_info: SessionInfo,
}

#[derive(Debug, Deserialize)]
pub struct SessionInfo {
    pub next_sender_seq_number: u64,
    pub next_target_seq_number: u64,
    pub status: String,
}

pub async fn run(cli: Cli) -> Result<()> {
    let client = reqwest::Client::new();
    let base_url = cli.url.trim_end_matches('/');

    match cli.command {
        Command::Health => {
            let url = format!("{}/api/health", base_url);
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to send health request")?;

            let status = response.status();
            let health: HealthResponse = response
                .json()
                .await
                .context("Failed to parse health response")?;

            println!("{} {}", "Status:".bold(), status.to_string().bright_blue());
            println!("{} {}", "Health:".bold(), health.status.green());
        }
        Command::SessionInfo => {
            let url = format!("{}/api/session-info", base_url);
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to send session-info request")?;

            let status = response.status();
            let info: SessionInfoResponse = response
                .json()
                .await
                .context("Failed to parse session-info response")?;

            println!("{} {}", "Status:".bold(), status.to_string().bright_blue());
            println!("{}", "Session Info:".bold().underline());
            println!(
                "  {}: {}",
                "Next Sender Seq Number".cyan(),
                info.session_info.next_sender_seq_number
            );
            println!(
                "  {}: {}",
                "Next Target Seq Number".cyan(),
                info.session_info.next_target_seq_number
            );
            println!(
                "  {}: {}",
                "Status".cyan(),
                info.session_info.status.yellow()
            );
        }
        Command::Reset => {
            let url = format!("{}/api/reset", base_url);
            let response = client
                .post(&url)
                .send()
                .await
                .context("Failed to send reset request")?;

            let status = response.status();
            if status.is_success() {
                println!("{} {}", "Status:".bold(), status.to_string().bright_blue());
                println!("{}", "Reset requested successfully".green());
            } else {
                let text = response.text().await.unwrap_or_default();
                anyhow::bail!(
                    "{} with status {}: {}",
                    "Reset request failed".red(),
                    status,
                    text
                );
            }
        }
        Command::Shutdown => {
            let url = format!("{}/api/shutdown", base_url);
            let response = client
                .post(&url)
                .send()
                .await
                .context("Failed to send shutdown request")?;

            let status = response.status();
            if status.is_success() {
                println!("{} {}", "Status:".bold(), status.to_string().bright_blue());
                println!("{}", "Shutdown requested successfully".green());
            } else {
                let text = response.text().await.unwrap_or_default();
                anyhow::bail!(
                    "{} with status {}: {}",
                    "Shutdown request failed".red(),
                    status,
                    text
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_health_command_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "healthy"
            })))
            .mount(&mock_server)
            .await;

        let cli = Cli {
            url: mock_server.uri(),
            command: Command::Health,
        };

        let result = run(cli).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_session_info_command_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/session-info"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "session_info": {
                    "next_sender_seq_number": 42,
                    "next_target_seq_number": 99,
                    "status": "Active"
                }
            })))
            .mount(&mock_server)
            .await;

        let cli = Cli {
            url: mock_server.uri(),
            command: Command::SessionInfo,
        };

        let result = run(cli).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_reset_command_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/reset"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&mock_server)
            .await;

        let cli = Cli {
            url: mock_server.uri(),
            command: Command::Reset,
        };

        let result = run(cli).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_command_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/shutdown"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&mock_server)
            .await;

        let cli = Cli {
            url: mock_server.uri(),
            command: Command::Shutdown,
        };

        let result = run(cli).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_reset_command_handles_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/reset"))
            .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
            .mount(&mock_server)
            .await;

        let cli = Cli {
            url: mock_server.uri(),
            command: Command::Reset,
        };

        let result = run(cli).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Reset request failed")
        );
    }

    #[tokio::test]
    async fn test_shutdown_command_handles_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/shutdown"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal server error"))
            .mount(&mock_server)
            .await;

        let cli = Cli {
            url: mock_server.uri(),
            command: Command::Shutdown,
        };

        let result = run(cli).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Shutdown request failed")
        );
    }

    #[tokio::test]
    async fn test_health_command_handles_invalid_json() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/health"))
            .respond_with(ResponseTemplate::new(200).set_body_string("invalid json"))
            .mount(&mock_server)
            .await;

        let cli = Cli {
            url: mock_server.uri(),
            command: Command::Health,
        };

        let result = run(cli).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse health response")
        );
    }

    #[tokio::test]
    async fn test_session_info_command_handles_invalid_json() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/session-info"))
            .respond_with(ResponseTemplate::new(200).set_body_string("invalid json"))
            .mount(&mock_server)
            .await;

        let cli = Cli {
            url: mock_server.uri(),
            command: Command::SessionInfo,
        };

        let result = run(cli).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse session-info response")
        );
    }

    #[tokio::test]
    async fn test_url_trailing_slash_is_trimmed() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "healthy"
            })))
            .mount(&mock_server)
            .await;

        let url_with_slash = format!("{}/", mock_server.uri());
        let cli = Cli {
            url: url_with_slash,
            command: Command::Health,
        };

        let result = run(cli).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_session_info_with_different_status_values() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/session-info"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "session_info": {
                    "next_sender_seq_number": 1,
                    "next_target_seq_number": 1,
                    "status": "AwaitingLogon"
                }
            })))
            .mount(&mock_server)
            .await;

        let cli = Cli {
            url: mock_server.uri(),
            command: Command::SessionInfo,
        };

        let result = run(cli).await;
        assert!(result.is_ok());
    }
}
