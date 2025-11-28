use assert_cmd::assert::OutputAssertExt;
use assert_cmd::cargo_bin;
use predicates::prelude::*;
use std::process::Command;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_cli_health_command_integration() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "healthy"
        })))
        .mount(&mock_server)
        .await;

    let mut cmd = Command::new(cargo_bin!("hotfix"));
    cmd.arg("--url")
        .arg(mock_server.uri())
        .arg("health")
        .assert()
        .success()
        .stdout(predicate::str::contains("Status:"))
        .stdout(predicate::str::contains("healthy"));
}

#[tokio::test]
async fn test_cli_session_info_command_integration() {
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

    let mut cmd = Command::new(cargo_bin!("hotfix"));
    cmd.arg("--url")
        .arg(mock_server.uri())
        .arg("session-info")
        .assert()
        .success()
        .stdout(predicate::str::contains("Session Info:"))
        .stdout(predicate::str::contains("Next Sender Seq Number"))
        .stdout(predicate::str::contains("42"))
        .stdout(predicate::str::contains("Next Target Seq Number"))
        .stdout(predicate::str::contains("99"))
        .stdout(predicate::str::contains("Active"));
}

#[tokio::test]
async fn test_cli_reset_command_integration() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/reset"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&mock_server)
        .await;

    let mut cmd = Command::new(cargo_bin!("hotfix"));
    cmd.arg("--url")
        .arg(mock_server.uri())
        .arg("reset")
        .assert()
        .success()
        .stdout(predicate::str::contains("Reset requested successfully"));
}

#[tokio::test]
async fn test_cli_shutdown_command_integration() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/shutdown"))
        .and(body_json(serde_json::json!({"reconnect": true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&mock_server)
        .await;

    let mut cmd = Command::new(cargo_bin!("hotfix"));
    cmd.arg("--url")
        .arg(mock_server.uri())
        .arg("shutdown")
        .assert()
        .success()
        .stdout(predicate::str::contains("Shutdown requested successfully"));
}

#[tokio::test]
async fn test_cli_shutdown_command_with_reconnect_false() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/shutdown"))
        .and(body_json(serde_json::json!({"reconnect": false})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&mock_server)
        .await;

    let mut cmd = Command::new(cargo_bin!("hotfix"));
    cmd.arg("--url")
        .arg(mock_server.uri())
        .arg("shutdown")
        .arg("--reconnect")
        .arg("false")
        .assert()
        .success()
        .stdout(predicate::str::contains("Shutdown requested successfully"));
}

#[tokio::test]
async fn test_cli_help_command() {
    let mut cmd = Command::new(cargo_bin!("hotfix"));
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "CLI tool for managing hotfix sessions",
        ))
        .stdout(predicate::str::contains("health"))
        .stdout(predicate::str::contains("session-info"))
        .stdout(predicate::str::contains("reset"))
        .stdout(predicate::str::contains("shutdown"));
}

#[tokio::test]
async fn test_cli_error_handling() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/reset"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Not found"))
        .mount(&mock_server)
        .await;

    let mut cmd = Command::new(cargo_bin!("hotfix"));
    cmd.arg("--url")
        .arg(mock_server.uri())
        .arg("reset")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Reset request failed"));
}

#[tokio::test]
async fn test_cli_with_env_var() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "healthy"
        })))
        .mount(&mock_server)
        .await;

    let mut cmd = Command::new(cargo_bin!("hotfix"));
    cmd.env("HOTFIX_WEB_URL", mock_server.uri())
        .arg("health")
        .assert()
        .success()
        .stdout(predicate::str::contains("healthy"));
}
