use crate::transport::reader::ReaderRef;
use crate::transport::writer::WriterRef;

pub struct FixConnection {
    _writer: WriterRef,
    _reader: ReaderRef,
}

impl FixConnection {
    pub fn new(writer: WriterRef, reader: ReaderRef) -> Self {
        Self {
            _writer: writer,
            _reader: reader,
        }
    }
    pub fn get_writer(&self) -> WriterRef {
        self._writer.clone()
    }

    pub async fn run_until_disconnect(self) {
        self._reader.wait_for_disconnect().await
    }
}
