use std::io::BufReader;
use std::sync::Arc;
use std::{fs, io};

use rustls::ClientConfig;
use rustls::RootCertStore;
use rustls_pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector, client::TlsStream};

use crate::config::{SessionConfig, TlsConfig};
use crate::transport::tcp::create_tcp_connection;

pub async fn create_tcp_over_tls_connection(
    session_config: &SessionConfig,
) -> io::Result<TlsStream<TcpStream>> {
    let tls_config = session_config
        .tls_config
        .as_ref()
        .expect("TLS config must be present when creating TLS connection");
    let client_config = get_client_config(tls_config);
    let socket = create_tcp_connection(session_config).await?;
    wrap_stream(
        socket,
        session_config.connection_host.clone(),
        Arc::new(client_config),
    )
    .await
}

fn get_client_config(tls_config: &TlsConfig) -> ClientConfig {
    let root_store = get_root_store(tls_config);
    ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth()
}

fn get_root_store(tls_config: &TlsConfig) -> RootCertStore {
    match tls_config {
        TlsConfig::File {
            ca_certificate_path,
        } => {
            let mut root_store = RootCertStore::empty();
            let certs = load_certs_from_file(ca_certificate_path);
            root_store.add_parsable_certificates(certs);
            root_store
        }
        TlsConfig::Native => {
            let mut root_store = RootCertStore::empty();
            let native_certs = rustls_native_certs::load_native_certs();
            root_store.add_parsable_certificates(native_certs.certs);
            root_store
        }
        TlsConfig::Webpki => {
            RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned())
        }
    }
}

fn load_certs_from_file(filename: &str) -> Vec<CertificateDer<'static>> {
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
    let domain = ServerName::try_from(domain).unwrap();
    let stream = TlsConnector::from(config);
    stream.connect(domain, socket).await
}
