pub mod socket_reader;
pub mod socket_writer;
pub mod tcp;
pub mod tls;

use std::io;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::session::SessionRef;
use crate::{
    config::SessionConfig,
    message::FixMessage,
    transport::{
        FixConnection, socket_reader::spawn_socket_reader, socket_writer::spawn_socket_writer,
        tcp::create_tcp_connection, tls::create_tcp_over_tls_connection,
    },
};

/// Connect over TCP/TLS and return a FixConnection
pub async fn connect(
    config: &SessionConfig,
    session_ref: SessionRef<impl FixMessage>,
) -> io::Result<FixConnection> {
    let use_tls = config.tls_config.is_some();

    let conn = if use_tls {
        let stream = create_tcp_over_tls_connection(config).await?;
        _create_io_refs(session_ref.clone(), stream).await
    } else {
        let stream = create_tcp_connection(config).await?;
        _create_io_refs(session_ref.clone(), stream).await
    };

    Ok(conn)
}

async fn _create_io_refs<M, Stream>(session_ref: SessionRef<M>, stream: Stream) -> FixConnection
where
    M: FixMessage,
    Stream: AsyncRead + AsyncWrite + Send + 'static,
{
    let (reader, writer) = tokio::io::split(stream);

    let writer_ref = spawn_socket_writer(writer);
    let reader_ref = spawn_socket_reader(reader, session_ref);

    FixConnection::new(writer_ref, reader_ref)
}
