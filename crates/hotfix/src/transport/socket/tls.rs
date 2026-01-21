use std::fs;
use std::io::BufReader;
use std::sync::Arc;

use rustls::ClientConfig;
use rustls::RootCertStore;
use rustls_pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector, client::TlsStream};

use crate::config::TlsConfig;
use crate::transport::error::ConnectionResult;
use crate::transport::tcp::create_tcp_connection;

pub async fn create_tcp_over_tls_connection(
    host: String,
    port: u16,
    tls_config: &TlsConfig,
) -> ConnectionResult<TlsStream<TcpStream>> {
    let client_config = get_client_config(tls_config)?;
    let socket = create_tcp_connection(&host, port).await?;
    wrap_stream(socket, host, Arc::new(client_config)).await
}

/// Create a TLS client configuration from the given TLS config.
pub fn get_client_config(tls_config: &TlsConfig) -> ConnectionResult<ClientConfig> {
    let root_store = get_root_store(tls_config)?;
    let client_config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Ok(client_config)
}

fn get_root_store(tls_config: &TlsConfig) -> ConnectionResult<RootCertStore> {
    let store = match tls_config {
        TlsConfig::File {
            ca_certificate_path,
        } => {
            let mut root_store = RootCertStore::empty();
            let certs = load_certs_from_file(ca_certificate_path)?;
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
    };

    Ok(store)
}

fn load_certs_from_file(filename: &str) -> ConnectionResult<Vec<CertificateDer<'static>>> {
    let certfile = fs::File::open(filename)?;
    let mut reader = BufReader::new(certfile);
    let certs = rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?;

    Ok(certs)
}

pub async fn wrap_stream<S>(
    socket: S,
    domain: String,
    config: Arc<ClientConfig>,
) -> ConnectionResult<TlsStream<S>>
where
    S: 'static + AsyncRead + AsyncWrite + Send + Unpin,
{
    let domain = ServerName::try_from(domain)?;
    let stream = TlsConnector::from(config);
    Ok(stream.connect(domain, socket).await?)
}
