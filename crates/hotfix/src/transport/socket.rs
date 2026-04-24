pub mod socket_reader;
pub mod socket_writer;
pub mod tcp;
pub mod tls;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::message::OutboundMessage;
use crate::session::InternalSessionRef;
use crate::transport::error::ConnectionResult;
use crate::{
    config::SessionConfig,
    transport::{
        FixConnection, socket_reader::spawn_socket_reader, socket_writer::spawn_socket_writer,
        tcp::create_tcp_connection, tls::create_tcp_over_tls_connection,
    },
};

/// Connect over TCP/TLS and return a FixConnection
pub async fn connect(
    config: &SessionConfig,
    session_ref: InternalSessionRef<impl OutboundMessage>,
) -> ConnectionResult<FixConnection> {
    let conn = if let Some(tls_config) = config.tls_config.as_ref() {
        let stream = create_tcp_over_tls_connection(
            config.connection_host.to_owned(),
            config.connection_port,
            tls_config,
        )
        .await?;
        _create_io_refs(session_ref.clone(), stream).await
    } else {
        let stream = create_tcp_connection(&config.connection_host, config.connection_port).await?;
        _create_io_refs(session_ref.clone(), stream).await
    };

    Ok(conn)
}

async fn _create_io_refs<Outbound, Stream>(
    session_ref: InternalSessionRef<Outbound>,
    stream: Stream,
) -> FixConnection
where
    Outbound: OutboundMessage,
    Stream: AsyncRead + AsyncWrite + Send + 'static,
{
    let (reader, writer) = tokio::io::split(stream);

    let (writer_ref, writer_exit) = spawn_socket_writer(writer);
    let reader_ref = spawn_socket_reader(reader, session_ref);

    FixConnection::new(writer_ref, reader_ref, writer_exit)
}
