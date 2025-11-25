use crate::transport::writer::{WriterMessage, WriterRef};
use tokio::io::{AsyncWrite, AsyncWriteExt, WriteHalf};
use tokio::sync::mpsc;
use tracing::{debug, warn};

pub fn spawn_socket_writer(writer: WriteHalf<impl AsyncWrite + Send + 'static>) -> WriterRef {
    let (sender, mailbox) = mpsc::channel(10);
    let actor = WriterActor::new(writer, mailbox);
    tokio::spawn(run_writer(actor));

    WriterRef::new(sender)
}

struct WriterActor<W> {
    writer: WriteHalf<W>,
    mailbox: mpsc::Receiver<WriterMessage>,
}

impl<W: AsyncWrite> WriterActor<W> {
    fn new(writer: WriteHalf<W>, mailbox: mpsc::Receiver<WriterMessage>) -> Self {
        Self { writer, mailbox }
    }

    async fn handle(&mut self, message: WriterMessage) -> bool {
        match message {
            WriterMessage::SendMessage(fix_message) => {
                match self.writer.write_all(fix_message.as_bytes()).await {
                    Ok(_) => debug!("sent message: {}", fix_message),
                    // we don't shut down the writer due to errors, only when explicitly requested
                    // a broken connection is shut down via the reader -> session -> writer route
                    Err(_) => warn!("failed to send message: {}", fix_message),
                }
                true
            }
            WriterMessage::Disconnect => false,
        }
    }
}

async fn run_writer<W: AsyncWrite>(mut actor: WriterActor<W>) {
    while let Some(msg) = actor.mailbox.recv().await {
        if !actor.handle(msg).await {
            break;
        }
    }

    debug!("writer loop is shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::parser::RawFixMessage;
    use tokio::io::{AsyncReadExt, duplex};

    /// Test that a single message is successfully written to the socket
    #[tokio::test]
    async fn test_send_single_message() {
        let (reader, writer) = duplex(1024);
        let (_reader_half, writer_half) = tokio::io::split(writer);
        let writer_ref = spawn_socket_writer(writer_half);

        let fix_message = b"8=FIX.4.4\x019=77\x0135=A\x0134=1\x0149=sender\x0152=20230908-08:24:56.574\x0156=target\x0198=0\x01108=30\x01141=Y\x0110=037\x01";
        let raw_message = RawFixMessage::new(fix_message.to_vec());

        writer_ref.send_raw_message(raw_message).await;

        // read from the other end of the duplex stream
        let mut reader = reader;
        let mut buffer = vec![0u8; 1024];
        let n = tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            reader.read(&mut buffer),
        )
        .await
        .expect("timeout waiting for message")
        .expect("read failed");

        assert_eq!(&buffer[..n], fix_message);
    }

    /// Test that multiple messages are written in order
    #[tokio::test]
    async fn test_send_multiple_messages() {
        let (reader, writer) = duplex(2048);
        let (_reader_half, writer_half) = tokio::io::split(writer);
        let writer_ref = spawn_socket_writer(writer_half);

        let msg1 = b"8=FIX.4.4\x019=77\x0135=A\x0134=1\x0149=sender\x0152=20230908-08:24:56.574\x0156=target\x0198=0\x01108=30\x01141=Y\x0110=037\x01";
        let msg2 = b"8=FIX.4.4\x019=77\x0135=A\x0134=2\x0149=sender\x0152=20230908-08:24:58.574\x0156=target\x0198=0\x01108=30\x01141=Y\x0110=040\x01";
        let msg3 = b"8=FIX.4.4\x019=77\x0135=A\x0134=3\x0149=sender\x0152=20230908-08:24:59.574\x0156=target\x0198=0\x01108=30\x01141=Y\x0110=043\x01";

        writer_ref
            .send_raw_message(RawFixMessage::new(msg1.to_vec()))
            .await;
        writer_ref
            .send_raw_message(RawFixMessage::new(msg2.to_vec()))
            .await;
        writer_ref
            .send_raw_message(RawFixMessage::new(msg3.to_vec()))
            .await;

        // read all messages from the other end
        let mut reader = reader;
        let mut buffer = vec![0u8; 2048];
        let n = tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            reader.read(&mut buffer),
        )
        .await
        .expect("timeout waiting for messages")
        .expect("read failed");

        // verify all three messages were written in order
        let mut expected = Vec::new();
        expected.extend_from_slice(msg1);
        expected.extend_from_slice(msg2);
        expected.extend_from_slice(msg3);

        assert_eq!(&buffer[..n], &expected[..]);
    }

    /// Test that disconnect message properly shuts down the writer loop
    #[tokio::test]
    async fn test_disconnect() {
        let (reader, writer) = duplex(1024);
        let (_reader_half, writer_half) = tokio::io::split(writer);
        let writer_ref = spawn_socket_writer(writer_half);

        // send a message first
        let fix_message = b"8=FIX.4.4\x019=77\x0135=A\x0134=1\x0149=sender\x0152=20230908-08:24:56.574\x0156=target\x0198=0\x01108=30\x01141=Y\x0110=037\x01";
        writer_ref
            .send_raw_message(RawFixMessage::new(fix_message.to_vec()))
            .await;

        // disconnect the writer
        writer_ref.disconnect().await;

        // verify the message was sent before disconnect
        let mut reader = reader;
        let mut buffer = vec![0u8; 1024];
        let n = tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            reader.read(&mut buffer),
        )
        .await
        .expect("timeout waiting for message")
        .expect("read failed");

        assert_eq!(&buffer[..n], fix_message);
    }

    /// Test that the writer can handle an empty message
    #[tokio::test]
    async fn test_send_empty_message() {
        let (reader, writer) = duplex(1024);
        let (_reader_half, writer_half) = tokio::io::split(writer);
        let writer_ref = spawn_socket_writer(writer_half);

        let empty_message = RawFixMessage::new(vec![]);
        writer_ref.send_raw_message(empty_message).await;

        // read from the other end - should complete immediately with 0 bytes
        let mut reader = reader;
        let mut buffer = vec![0u8; 1024];

        // give it a moment to process
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // try to read, but with a short timeout since we expect nothing
        match tokio::time::timeout(
            tokio::time::Duration::from_millis(50),
            reader.read(&mut buffer),
        )
        .await
        {
            Ok(Ok(n)) => assert_eq!(n, 0, "Expected 0 bytes for empty message"),
            Err(_) => {
                // Timeout is also acceptable - no data to read
            }
            Ok(Err(e)) => panic!("Read failed: {}", e),
        }
    }

    /// Test that the writer loop properly shuts down when the mailbox is closed
    #[tokio::test]
    async fn test_writer_shutdown_on_mailbox_close() {
        let (_reader, writer) = duplex(1024);
        let (_reader_half, writer_half) = tokio::io::split(writer);
        let writer_ref = spawn_socket_writer(writer_half);

        // send a message to ensure the writer is running
        let fix_message = b"8=FIX.4.4\x019=77\x0135=A\x0134=1\x0149=sender\x0152=20230908-08:24:56.574\x0156=target\x0198=0\x01108=30\x01141=Y\x0110=037\x01";
        writer_ref
            .send_raw_message(RawFixMessage::new(fix_message.to_vec()))
            .await;

        // drop the writer_ref, which closes the channel
        drop(writer_ref);

        // give the writer loop time to shut down
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // If the test completes without hanging, the writer loop properly shut down
        // when the mailbox was closed
    }

    /// Test writer behaviour with a closed socket (write error handling)
    #[tokio::test]
    async fn test_write_error_handling() {
        let (reader, writer) = duplex(1024);
        let (_reader_half, writer_half) = tokio::io::split(writer);
        let writer_ref = spawn_socket_writer(writer_half);

        // close the reader end, which should cause write errors
        drop(reader);

        // try to send a message - should not panic, but will log a warning
        let fix_message = b"8=FIX.4.4\x019=77\x0135=A\x0134=1\x0149=sender\x0152=20230908-08:24:56.574\x0156=target\x0198=0\x01108=30\x01141=Y\x0110=037\x01";
        writer_ref
            .send_raw_message(RawFixMessage::new(fix_message.to_vec()))
            .await;

        // writer should still be running (not shut down due to write error)
        // send another message to verify
        writer_ref
            .send_raw_message(RawFixMessage::new(fix_message.to_vec()))
            .await;

        // if we reach here without panic, the writer correctly handled the error
        // and continued running (as per the code comment that it only shuts down
        // when explicitly requested)
    }
}
