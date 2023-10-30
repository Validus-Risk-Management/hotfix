use std::io;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::actors::socket_reader::ReaderRef;
use crate::actors::socket_writer::WriterRef;
use crate::config::SessionConfig;
use crate::message::FixMessage;
use crate::session::SessionRef;
use crate::transport::tcp::create_tcp_connection;
use crate::transport::tls::create_tcp_over_tls_connection;

pub struct FixConnection {
    _writer: WriterRef,
    _reader: ReaderRef,
}

impl FixConnection {
    pub async fn connect(
        config: &SessionConfig,
        session_ref: SessionRef<impl FixMessage>,
    ) -> io::Result<Self> {
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

    pub fn get_writer(&self) -> WriterRef {
        self._writer.clone()
    }

    pub async fn run_until_disconnect(self) {
        self._reader.wait_for_disconnect().await
    }
}

async fn _create_io_refs<M, Stream>(session_ref: SessionRef<M>, stream: Stream) -> FixConnection
where
    M: FixMessage,
    Stream: AsyncRead + AsyncWrite + Send + 'static,
{
    let (reader, writer) = tokio::io::split(stream);

    let writer_ref = WriterRef::new(writer);
    let reader_ref = ReaderRef::new(reader, session_ref);

    FixConnection {
        _writer: writer_ref,
        _reader: reader_ref,
    }
}
