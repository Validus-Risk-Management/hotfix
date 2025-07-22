use std::io;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::config::SessionConfig;
use crate::message::FixMessage;
use crate::session::SessionRef;
use crate::transport::actor::WriterModel;
use crate::transport::socket_reader::ReaderRef;
use crate::transport::socket_writer::WriterRef;
use crate::transport::tcp::create_tcp_connection;
use crate::transport::tls::create_tcp_over_tls_connection;

pub struct FixConnection<W: WriterModel> {
    _writer: W,
    _reader: ReaderRef,
}

impl<W: WriterModel> FixConnection<W> {
    pub fn get_writer(&self) -> W {
        self._writer.clone()
    }

    pub async fn run_until_disconnect(self) {
        self._reader.wait_for_disconnect().await
    }
}

impl From<(WriterRef, ReaderRef)> for FixConnection<WriterRef> {
    fn from(refs: (WriterRef, ReaderRef)) -> Self {
        let (_writer, _reader) = refs;
        FixConnection { _writer, _reader }
    }
}

/// Spawn a TCP or TLS FIX Connection
pub async fn build_connection(
    config: &SessionConfig,
    session_ref: SessionRef<impl FixMessage>,
) -> io::Result<FixConnection<WriterRef>> {
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

async fn _create_io_refs<M, Stream>(
    session_ref: SessionRef<M>,
    stream: Stream,
) -> FixConnection<WriterRef>
where
    M: FixMessage,
    Stream: AsyncRead + AsyncWrite + Send + 'static,
{
    let (reader, writer) = tokio::io::split(stream);

    let writer_ref = WriterRef::new(writer);
    let reader_ref = ReaderRef::new(reader, session_ref);

    FixConnection::from((writer_ref, reader_ref))
}
