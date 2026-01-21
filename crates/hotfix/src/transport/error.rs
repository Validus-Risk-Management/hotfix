/// Errors that may occur when establishing a connection.
#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("IO error")]
    IOError(#[from] std::io::Error),

    #[error("Invalid DNS name")]
    InvalidDnsName(#[from] rustls_pki_types::InvalidDnsNameError),
}

pub type ConnectionResult<T> = Result<T, ConnectionError>;
