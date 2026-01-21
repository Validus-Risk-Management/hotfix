//! Integration tests for the transport layer's `connect` function.
//!
//! These tests verify that `hotfix::transport::socket::connect` correctly
//! establishes connections over TCP and TLS.

use crate::helpers::{
    MinimalApplication, MinimalMessage, TestCertificates, TestTcpServer, TestTlsServer,
};
use hotfix::config::{SessionConfig, TlsConfig};
use hotfix::session::InternalSessionRef;
use hotfix::store::in_memory::InMemoryMessageStore;
use hotfix::transport::socket::connect;

fn create_session_config(host: &str, port: u16, tls_config: Option<TlsConfig>) -> SessionConfig {
    SessionConfig {
        begin_string: "FIX.4.4".to_string(),
        sender_comp_id: "TEST_SENDER".to_string(),
        target_comp_id: "TEST_TARGET".to_string(),
        data_dictionary_path: None,
        connection_host: host.to_string(),
        connection_port: port,
        tls_config,
        heartbeat_interval: 30,
        logon_timeout: 10,
        logout_timeout: 2,
        reconnect_interval: 30,
        reset_on_logon: false,
        schedule: None,
    }
}

fn create_session_ref() -> InternalSessionRef<MinimalMessage> {
    let store = InMemoryMessageStore::default();
    let app = MinimalApplication;
    InternalSessionRef::new(create_session_config("", 0, None), app, store)
        .expect("Failed to create session ref")
}

#[tokio::test]
async fn test_connect_with_tls_config() {
    // Generate certificates and start TLS server
    let certs = TestCertificates::generate(&["localhost"]);
    let ca_file = certs.write_ca_to_temp_file();
    let server = TestTlsServer::start(&certs).await;

    // Create session config with TLS
    let tls_config = TlsConfig::File {
        ca_certificate_path: ca_file.path().to_string_lossy().to_string(),
    };
    let config = create_session_config("localhost", server.port(), Some(tls_config));

    // Create a session ref
    let session_ref = create_session_ref();

    // Call connect - this should establish a TLS connection and return a FixConnection
    let result = connect(&config, session_ref).await;

    assert!(
        result.is_ok(),
        "connect() with TLS config should succeed: {:?}",
        result.err()
    );

    let connection = result.unwrap();

    // Verify we got a valid connection by checking we can get a writer
    let _writer = connection.get_writer();

    server.shutdown().await;
}

#[tokio::test]
async fn test_connect_without_tls_config() {
    // Start a plain TCP server
    let server = TestTcpServer::start().await;

    // Create session config without TLS
    let config = create_session_config("127.0.0.1", server.port(), None);

    // Create a session ref
    let session_ref = create_session_ref();

    // Call connect - this should establish a plain TCP connection
    let result = connect(&config, session_ref).await;

    assert!(
        result.is_ok(),
        "connect() without TLS config should succeed: {:?}",
        result.err()
    );

    let connection = result.unwrap();

    // Verify we got a valid connection
    let _writer = connection.get_writer();

    server.shutdown().await;
}

#[tokio::test]
async fn test_connect_with_tls_fails_on_bad_certificate() {
    // Generate two different certificate sets
    let server_certs = TestCertificates::generate(&["localhost"]);
    let client_certs = TestCertificates::generate(&["localhost"]); // Different CA
    let ca_file = client_certs.write_ca_to_temp_file();

    // Start server with its own certificates
    let server = TestTlsServer::start(&server_certs).await;

    // Client trusts a different CA - connection should fail
    let tls_config = TlsConfig::File {
        ca_certificate_path: ca_file.path().to_string_lossy().to_string(),
    };
    let config = create_session_config("localhost", server.port(), Some(tls_config));

    let session_ref = create_session_ref();

    let result = connect(&config, session_ref).await;

    assert!(result.is_err(), "connect() with untrusted CA should fail");

    server.shutdown().await;
}

#[tokio::test]
async fn test_connect_fails_when_server_not_running() {
    // Try to connect to a port where nothing is listening
    let config = create_session_config("127.0.0.1", 59998, None);

    let session_ref = create_session_ref();

    let result = connect(&config, session_ref).await;

    assert!(
        result.is_err(),
        "connect() to non-existent server should fail"
    );
}
