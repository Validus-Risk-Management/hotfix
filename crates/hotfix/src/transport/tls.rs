use std::io::BufReader;
use std::sync::Arc;
use std::{fs, io};

use pki_types::CertificateDer;
use rustls::ClientConfig;
use rustls::{RootCertStore, ServerName};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::{client::TlsStream, TlsConnector};

use crate::config::SessionConfig;
use crate::transport::tcp::create_tcp_connection;

pub async fn create_tcp_over_tls_connection(
    session_config: &SessionConfig,
) -> io::Result<TlsStream<TcpStream>> {
    let client_config = get_client_config(session_config);
    let socket = create_tcp_connection(session_config).await?;
    wrap_stream(
        socket,
        session_config.connection_host.clone(),
        Arc::new(client_config),
    )
    .await
}

fn get_client_config(session_config: &SessionConfig) -> ClientConfig {
    let root_store = get_root_store(
        &session_config
            .tls_config
            .clone()
            .unwrap()
            .ca_certificate_path,
    );
    ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_store)
        .with_no_client_auth()
}

fn get_root_store(ca_certificate_path: &str) -> RootCertStore {
    let mut root_store = RootCertStore::empty();
    let certs = load_certs(ca_certificate_path);
    root_store.add_parsable_certificates(&certs);

    root_store
}

fn load_certs(filename: &str) -> Vec<CertificateDer<'static>> {
    let certfile = fs::File::open(filename).expect("certificate file to be open");
    let mut reader = BufReader::new(certfile);
    rustls_pemfile::certs(&mut reader)
        .map(|result| result.unwrap())
        .collect()
}

pub async fn wrap_stream<S>(
    socket: S,
    domain: String,
    config: Arc<ClientConfig>,
) -> io::Result<TlsStream<S>>
where
    S: 'static + AsyncRead + AsyncWrite + Send + Unpin,
{
    let domain = ServerName::try_from(domain.as_str()).unwrap();
    let stream = TlsConnector::from(config);
    stream.connect(domain, socket).await
}
