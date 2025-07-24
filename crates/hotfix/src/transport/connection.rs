use crate::transport::reader::ReaderRef;
use crate::transport::writer::WriterRef;

pub struct FixConnection {
    writer: WriterRef,
    reader: ReaderRef,
}

impl FixConnection {
    pub fn new(writer: WriterRef, reader: ReaderRef) -> Self {
        Self { writer, reader }
    }
    pub fn get_writer(&self) -> WriterRef {
        self.writer.clone()
    }

    pub async fn run_until_disconnect(self) {
        self.reader.wait_for_disconnect().await
    }
}
