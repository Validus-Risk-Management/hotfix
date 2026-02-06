//! Test helpers for connection-related integration tests.
//!
//! Provides utilities for generating test certificates, spinning up local servers,
//! and minimal Application implementations for testing.

use std::io::Write;
use std::net::SocketAddr;
use std::sync::{Arc, Once};

use hotfix::Application;
use hotfix::application::{InboundDecision, OutboundDecision};
use hotfix::message::OutboundMessage;
use hotfix_message::message::Message;
use rcgen::{CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose, SanType};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_rustls::TlsAcceptor;

static CRYPTO_PROVIDER_INIT: Once = Once::new();

/// Initialize the rustls crypto provider for tests.
/// This must be called before any TLS operations.
pub fn init_crypto_provider() {
    CRYPTO_PROVIDER_INIT.call_once(|| {
        rustls::crypto::ring::default_provider()
            .install_default()
            .expect("Failed to install crypto provider");
    });
}

/// A set of test certificates including a CA and server certificate.
pub struct TestCertificates {
    /// The CA certificate in PEM format.
    pub ca_cert_pem: String,
    /// The server certificate in DER format.
    pub server_cert_der: CertificateDer<'static>,
    /// The server private key in DER format.
    pub server_key_der: PrivateKeyDer<'static>,
}

impl TestCertificates {
    /// Generate a new set of test certificates.
    ///
    /// Creates a self-signed CA certificate and a server certificate signed by that CA.
    /// The server certificate will be valid for the specified domain names.
    pub fn generate(domains: &[&str]) -> Self {
        // Ensure crypto provider is initialized
        init_crypto_provider();

        // Generate CA key pair
        let ca_key_pair = KeyPair::generate().expect("Failed to generate CA key pair");

        // Create CA certificate parameters
        let mut ca_params = CertificateParams::default();
        ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "Test CA");
        ca_params
            .distinguished_name
            .push(DnType::OrganizationName, "Test Organization");
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        // Generate the CA certificate
        let ca_cert = ca_params
            .self_signed(&ca_key_pair)
            .expect("Failed to generate CA certificate");

        // Generate server key pair
        let server_key_pair = KeyPair::generate().expect("Failed to generate server key pair");

        // Create server certificate parameters
        let mut server_params = CertificateParams::default();
        server_params
            .distinguished_name
            .push(DnType::CommonName, *domains.first().unwrap_or(&"localhost"));
        server_params
            .distinguished_name
            .push(DnType::OrganizationName, "Test Organization");

        // Add Subject Alternative Names for all domains
        server_params.subject_alt_names = domains
            .iter()
            .map(|d| {
                // Try to parse as IP address first
                if let Ok(ip) = d.parse::<std::net::IpAddr>() {
                    SanType::IpAddress(ip)
                } else {
                    SanType::DnsName((*d).try_into().expect("Invalid DNS name"))
                }
            })
            .collect();

        server_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];

        // Sign the server certificate with the CA
        let server_cert = server_params
            .signed_by(&server_key_pair, &ca_cert, &ca_key_pair)
            .expect("Failed to sign server certificate");

        TestCertificates {
            ca_cert_pem: ca_cert.pem(),
            server_cert_der: CertificateDer::from(server_cert.der().to_vec()),
            server_key_der: PrivateKeyDer::try_from(server_key_pair.serialize_der())
                .expect("Failed to convert server key"),
        }
    }

    /// Write the CA certificate to a temporary file and return the file.
    ///
    /// The returned `NamedTempFile` will keep the file alive as long as it exists.
    pub fn write_ca_to_temp_file(&self) -> NamedTempFile {
        let mut temp_file =
            NamedTempFile::new().expect("Failed to create temporary file for CA cert");
        temp_file
            .write_all(self.ca_cert_pem.as_bytes())
            .expect("Failed to write CA cert to temp file");
        temp_file.flush().expect("Failed to flush temp file");
        temp_file
    }

    /// Create a rustls ServerConfig from this certificate set.
    pub fn server_config(&self) -> ServerConfig {
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![self.server_cert_der.clone()],
                self.server_key_der.clone_key(),
            )
            .expect("Failed to create server config")
    }
}

/// A test TLS server that can be used for integration testing.
pub struct TestTlsServer {
    /// The address the server is listening on.
    pub addr: SocketAddr,
    /// Channel to signal the server to shut down.
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Handle to the server task.
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl TestTlsServer {
    /// Start a new TLS server with the given certificates.
    ///
    /// The server will listen on a random available port on localhost.
    /// It echoes back any data it receives.
    pub async fn start(certs: &TestCertificates) -> Self {
        Self::start_with_behavior(certs, ServerBehavior::Echo).await
    }

    /// Start a new TLS server with specified behavior.
    pub async fn start_with_behavior(certs: &TestCertificates, behavior: ServerBehavior) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind TCP listener");
        let addr = listener.local_addr().expect("Failed to get local address");

        let server_config = certs.server_config();
        let acceptor = TlsAcceptor::from(Arc::new(server_config));

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        let task_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((tcp_stream, _peer_addr)) => {
                                let acceptor = acceptor.clone();
                                tokio::spawn(async move {
                                    match behavior {
                                        ServerBehavior::Echo => {
                                            if let Ok(mut tls_stream) = acceptor.accept(tcp_stream).await {
                                                let mut buf = [0u8; 1024];
                                                while let Ok(n) = tls_stream.read(&mut buf).await {
                                                    if n == 0 {
                                                        break;
                                                    }
                                                    let _ = tls_stream.write_all(&buf[..n]).await;
                                                }
                                            }
                                        }
                                        ServerBehavior::CloseImmediately => {
                                            // Just drop the connection without completing TLS handshake
                                            drop(tcp_stream);
                                        }
                                    }
                                });
                            }
                            Err(_) => break,
                        }
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }
        });

        TestTlsServer {
            addr,
            shutdown_tx: Some(shutdown_tx),
            task_handle: Some(task_handle),
        }
    }

    /// Get the port the server is listening on.
    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Shutdown the server gracefully.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.task_handle.take() {
            let _ = handle.await;
        }
    }
}

impl Drop for TestTlsServer {
    fn drop(&mut self) {
        // Signal shutdown if not already done
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// Describes how the test server should behave.
#[derive(Clone, Copy, Debug)]
pub enum ServerBehavior {
    /// Echo back any received data (normal operation).
    Echo,
    /// Close the connection immediately after TCP accept, before TLS handshake.
    CloseImmediately,
}

/// A test TCP server (without TLS) for integration testing.
pub struct TestTcpServer {
    pub addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl TestTcpServer {
    /// Start a new TCP echo server.
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind TCP listener");
        let addr = listener.local_addr().expect("Failed to get local address");

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        let task_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((mut tcp_stream, _)) => {
                                tokio::spawn(async move {
                                    let mut buf = [0u8; 1024];
                                    while let Ok(n) = tcp_stream.read(&mut buf).await {
                                        if n == 0 {
                                            break;
                                        }
                                        let _ = tcp_stream.write_all(&buf[..n]).await;
                                    }
                                });
                            }
                            Err(_) => break,
                        }
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }
        });

        TestTcpServer {
            addr,
            shutdown_tx: Some(shutdown_tx),
            task_handle: Some(task_handle),
        }
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.task_handle.take() {
            let _ = handle.await;
        }
    }
}

impl Drop for TestTcpServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// A minimal message type for testing that doesn't require fix44 types.
#[derive(Debug, Clone)]
pub struct MinimalMessage;

impl OutboundMessage for MinimalMessage {
    fn write(&self, _msg: &mut Message) {
        // No-op for minimal test message
    }

    fn message_type(&self) -> &str {
        "0" // Heartbeat type, simplest message
    }
}

/// A minimal Application implementation for testing transport connectivity.
pub struct MinimalApplication;

#[async_trait::async_trait]
impl Application for MinimalApplication {
    type Outbound = MinimalMessage;

    async fn on_outbound_message(&self, _msg: &MinimalMessage) -> OutboundDecision {
        OutboundDecision::Send
    }

    async fn on_inbound_message(&self, _msg: &Message) -> InboundDecision {
        InboundDecision::Accept
    }

    async fn on_logout(&mut self, _reason: &str) {}

    async fn on_logon(&mut self) {}
}
