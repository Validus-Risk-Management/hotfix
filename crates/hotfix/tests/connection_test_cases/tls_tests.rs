//! Integration tests for TLS transport functionality.
//!
//! These tests verify the TLS connection logic in `crates/hotfix/src/transport/socket/tls.rs`.

use std::sync::Arc;

use hotfix::config::TlsConfig;
use hotfix::transport::error::ConnectionError;
use hotfix::transport::socket::tls::{create_tcp_over_tls_connection, wrap_stream};
use rustls::ClientConfig;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::helpers::{ServerBehavior, TestCertificates, TestTlsServer, init_crypto_provider};

#[tokio::test]
async fn test_tls_connection_with_file_config_succeeds() {
    // Generate test certificates valid for localhost
    let certs = TestCertificates::generate(&["localhost"]);
    let ca_file = certs.write_ca_to_temp_file();

    // Start a TLS server
    let server = TestTlsServer::start(&certs).await;

    // Create TLS config using the CA file
    let tls_config = TlsConfig::File {
        ca_certificate_path: ca_file.path().to_string_lossy().to_string(),
    };

    // Connect to the server
    let mut stream =
        create_tcp_over_tls_connection("localhost".to_string(), server.port(), &tls_config)
            .await
            .expect("TLS connection should succeed");

    // Verify the connection works by sending and receiving data
    let test_data = b"Hello, TLS!";
    stream
        .write_all(test_data)
        .await
        .expect("Write should succeed");

    let mut buf = vec![0u8; test_data.len()];
    stream
        .read_exact(&mut buf)
        .await
        .expect("Read should succeed");

    assert_eq!(&buf, test_data);

    server.shutdown().await;
}

#[tokio::test]
async fn test_tls_connection_with_ip_address() {
    // Generate test certificates valid for 127.0.0.1
    let certs = TestCertificates::generate(&["127.0.0.1"]);
    let ca_file = certs.write_ca_to_temp_file();

    // Start a TLS server
    let server = TestTlsServer::start(&certs).await;

    // Create TLS config
    let tls_config = TlsConfig::File {
        ca_certificate_path: ca_file.path().to_string_lossy().to_string(),
    };

    // Connect using IP address
    let mut stream =
        create_tcp_over_tls_connection("127.0.0.1".to_string(), server.port(), &tls_config)
            .await
            .expect("TLS connection with IP should succeed");

    // Verify connection works
    let test_data = b"IP address test";
    stream
        .write_all(test_data)
        .await
        .expect("Write should succeed");

    let mut buf = vec![0u8; test_data.len()];
    stream
        .read_exact(&mut buf)
        .await
        .expect("Read should succeed");

    assert_eq!(&buf, test_data);

    server.shutdown().await;
}

#[tokio::test]
async fn test_wrap_stream_with_valid_config() {
    // Generate test certificates
    let certs = TestCertificates::generate(&["localhost"]);
    let ca_file = certs.write_ca_to_temp_file();

    // Start a TLS server
    let server = TestTlsServer::start(&certs).await;

    // Establish raw TCP connection
    let tcp_stream = TcpStream::connect(server.addr)
        .await
        .expect("TCP connection should succeed");

    // Build client config manually using the CA
    let tls_config = TlsConfig::File {
        ca_certificate_path: ca_file.path().to_string_lossy().to_string(),
    };
    let client_config = hotfix::transport::socket::tls::get_client_config(&tls_config)
        .expect("Client config should be created");

    // Wrap the stream
    let mut tls_stream = wrap_stream(tcp_stream, "localhost".to_string(), Arc::new(client_config))
        .await
        .expect("wrap_stream should succeed");

    // Verify the wrapped stream works
    let test_data = b"Wrapped stream test";
    tls_stream
        .write_all(test_data)
        .await
        .expect("Write should succeed");

    let mut buf = vec![0u8; test_data.len()];
    tls_stream
        .read_exact(&mut buf)
        .await
        .expect("Read should succeed");

    assert_eq!(&buf, test_data);

    server.shutdown().await;
}

#[tokio::test]
async fn test_tls_connection_fails_with_untrusted_ca() {
    // Generate two separate certificate sets - server will use one, client trusts another
    let server_certs = TestCertificates::generate(&["localhost"]);
    let untrusted_certs = TestCertificates::generate(&["localhost"]);
    let untrusted_ca_file = untrusted_certs.write_ca_to_temp_file();

    // Start server with its own certificates
    let server = TestTlsServer::start(&server_certs).await;

    // Client trusts a different CA
    let tls_config = TlsConfig::File {
        ca_certificate_path: untrusted_ca_file.path().to_string_lossy().to_string(),
    };

    // Connection should fail due to untrusted certificate
    let result =
        create_tcp_over_tls_connection("localhost".to_string(), server.port(), &tls_config).await;

    assert!(result.is_err(), "Connection should fail with untrusted CA");
    match result.unwrap_err() {
        ConnectionError::IOError(e) => {
            let error_string = e.to_string();
            assert!(
                error_string.contains("certificate") || error_string.contains("invalid"),
                "Error should mention certificate issue: {error_string}"
            );
        }
        other => panic!("Expected IOError, got: {other:?}"),
    }

    server.shutdown().await;
}

#[tokio::test]
async fn test_tls_connection_fails_with_hostname_mismatch() {
    // Generate certificate only valid for "other-host.example.com"
    let certs = TestCertificates::generate(&["other-host.example.com"]);
    let ca_file = certs.write_ca_to_temp_file();

    // Start server
    let server = TestTlsServer::start(&certs).await;

    // Try to connect using "localhost" - hostname won't match certificate
    let tls_config = TlsConfig::File {
        ca_certificate_path: ca_file.path().to_string_lossy().to_string(),
    };

    let result =
        create_tcp_over_tls_connection("localhost".to_string(), server.port(), &tls_config).await;

    assert!(
        result.is_err(),
        "Connection should fail with hostname mismatch"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn test_wrap_stream_invalid_dns_name_empty_string() {
    init_crypto_provider();

    // Create a mock TCP stream (we won't actually connect, just test DNS name validation)
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Bind should succeed");
    let addr = listener.local_addr().expect("Should have local addr");

    // Connect to our own listener
    let tcp_stream = TcpStream::connect(addr)
        .await
        .expect("TCP connect should succeed");

    // Create a minimal client config
    let client_config = ClientConfig::builder()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();

    // Try to wrap with empty domain name
    let result = wrap_stream(tcp_stream, "".to_string(), Arc::new(client_config)).await;

    assert!(result.is_err(), "Empty domain should fail");
    match result.unwrap_err() {
        ConnectionError::InvalidDnsName(_) => {}
        other => panic!("Expected InvalidDnsName error, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_wrap_stream_invalid_dns_name_formats() {
    init_crypto_provider();

    let invalid_domains = vec![
        "",               // Empty
        " ",              // Whitespace only
        "host name",      // Contains space
        "-invalid.com",   // Starts with hyphen
        "invalid-.com",   // Ends with hyphen
        "a]bad[name.com", // Invalid characters
    ];

    for domain in invalid_domains {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Bind should succeed");
        let addr = listener.local_addr().expect("Should have local addr");

        let tcp_stream = TcpStream::connect(addr)
            .await
            .expect("TCP connect should succeed");

        let client_config = ClientConfig::builder()
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth();

        let result = wrap_stream(tcp_stream, domain.to_string(), Arc::new(client_config)).await;

        assert!(
            result.is_err(),
            "Domain '{domain}' should fail DNS validation"
        );
        match result.unwrap_err() {
            ConnectionError::InvalidDnsName(_) => {}
            other => panic!("Expected InvalidDnsName error for '{domain}', got: {other:?}"),
        }
    }
}

#[tokio::test]
async fn test_tls_connection_to_nonexistent_server() {
    let certs = TestCertificates::generate(&["localhost"]);
    let ca_file = certs.write_ca_to_temp_file();

    let tls_config = TlsConfig::File {
        ca_certificate_path: ca_file.path().to_string_lossy().to_string(),
    };

    // Try to connect to a port where nothing is listening
    // Use a high port number that's unlikely to be in use
    let result = create_tcp_over_tls_connection("localhost".to_string(), 59999, &tls_config).await;

    assert!(
        result.is_err(),
        "Connection to nonexistent server should fail"
    );
    match result.unwrap_err() {
        ConnectionError::IOError(_) => {}
        other => panic!("Expected IOError, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_tls_connection_refused() {
    // Bind a port but don't accept connections, then close it
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Bind should succeed");
    let port = listener
        .local_addr()
        .expect("Should have local addr")
        .port();
    drop(listener); // Close the listener immediately

    let certs = TestCertificates::generate(&["127.0.0.1"]);
    let ca_file = certs.write_ca_to_temp_file();

    let tls_config = TlsConfig::File {
        ca_certificate_path: ca_file.path().to_string_lossy().to_string(),
    };

    // Try to connect to the closed port
    let result = create_tcp_over_tls_connection("127.0.0.1".to_string(), port, &tls_config).await;

    assert!(result.is_err(), "Connection to closed port should fail");
    match result.unwrap_err() {
        ConnectionError::IOError(_) => {}
        other => panic!("Expected IOError, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_tls_config_native_root_store() {
    init_crypto_provider();

    // Test that Native config successfully creates a client config
    let tls_config = TlsConfig::Native;
    let result = hotfix::transport::socket::tls::get_client_config(&tls_config);

    // Should succeed in creating the config (even if the root store may be empty on some systems)
    assert!(
        result.is_ok(),
        "Native root store config should be created successfully"
    );
}

#[tokio::test]
async fn test_tls_config_webpki_root_store() {
    init_crypto_provider();

    // Test that Webpki config successfully creates a client config
    let tls_config = TlsConfig::Webpki;
    let result = hotfix::transport::socket::tls::get_client_config(&tls_config);

    // Should succeed - webpki-roots provides bundled certificates
    assert!(
        result.is_ok(),
        "Webpki root store config should be created successfully"
    );
}

#[tokio::test]
async fn test_tls_config_file_with_nonexistent_path() {
    init_crypto_provider();

    let tls_config = TlsConfig::File {
        ca_certificate_path: "/nonexistent/path/to/ca.pem".to_string(),
    };

    let result = hotfix::transport::socket::tls::get_client_config(&tls_config);

    assert!(result.is_err(), "Nonexistent CA file should fail");
    match result.unwrap_err() {
        ConnectionError::IOError(_) => {}
        other => panic!("Expected IOError for nonexistent file, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_server_closes_after_tcp_accept() {
    let certs = TestCertificates::generate(&["localhost"]);
    let ca_file = certs.write_ca_to_temp_file();

    // Start server that closes connections immediately
    let server = TestTlsServer::start_with_behavior(&certs, ServerBehavior::CloseImmediately).await;

    let tls_config = TlsConfig::File {
        ca_certificate_path: ca_file.path().to_string_lossy().to_string(),
    };

    // Connection should fail when server closes during handshake
    let result =
        create_tcp_over_tls_connection("localhost".to_string(), server.port(), &tls_config).await;

    assert!(
        result.is_err(),
        "Connection should fail when server closes during handshake"
    );

    server.shutdown().await;
}
