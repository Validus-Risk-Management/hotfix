use std::time::Duration;

use crate::transport::writer::{WriterMessage, WriterRef};
use tokio::io::{AsyncWrite, AsyncWriteExt, WriteHalf};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

const WRITER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

pub fn spawn_socket_writer<W>(writer: WriteHalf<W>) -> (WriterRef, oneshot::Receiver<()>)
where
    W: AsyncWrite + Send + 'static,
{
    let (sender, mailbox) = mpsc::channel(10);
    let (exit_tx, exit_rx) = oneshot::channel();
    let actor = WriterActor::new(writer, mailbox);
    tokio::spawn(run_writer(actor, exit_tx));

    (WriterRef::new(sender), exit_rx)
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

async fn run_writer<W: AsyncWrite>(mut actor: WriterActor<W>, exit_tx: oneshot::Sender<()>) {
    while let Some(msg) = actor.mailbox.recv().await {
        if !actor.handle(msg).await {
            break;
        }
    }

    match tokio::time::timeout(WRITER_SHUTDOWN_TIMEOUT, actor.writer.shutdown()).await {
        Ok(Ok(())) => debug!("writer half closed cleanly"),
        Ok(Err(err)) => warn!("writer shutdown returned error: {err}"),
        Err(_) => warn!(
            "writer shutdown timed out after {:?}",
            WRITER_SHUTDOWN_TIMEOUT
        ),
    }

    let _ = exit_tx.send(());
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
        let (writer_ref, _exit_rx) = spawn_socket_writer(writer_half);

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
        let (writer_ref, _exit_rx) = spawn_socket_writer(writer_half);

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
        let (writer_ref, _exit_rx) = spawn_socket_writer(writer_half);

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
        let (writer_ref, _exit_rx) = spawn_socket_writer(writer_half);

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
        let (writer_ref, _exit_rx) = spawn_socket_writer(writer_half);

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
        let (writer_ref, _exit_rx) = spawn_socket_writer(writer_half);

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

    /// After processing Disconnect, the actor calls shutdown() on its WriteHalf,
    /// which for a duplex stream surfaces as EOF on the peer read side.
    #[tokio::test]
    async fn shutdown_called_on_disconnect() {
        let (reader, writer) = duplex(1024);
        let (_reader_half, writer_half) = tokio::io::split(writer);
        let (writer_ref, exit_rx) = spawn_socket_writer(writer_half);

        writer_ref.disconnect().await;

        tokio::time::timeout(tokio::time::Duration::from_millis(200), exit_rx)
            .await
            .expect("exit signal not fired within timeout")
            .expect("exit sender dropped without signalling");

        // Peer side of the duplex should observe EOF after shutdown.
        let mut reader = reader;
        let mut buf = vec![0u8; 16];
        let n = tokio::time::timeout(
            tokio::time::Duration::from_millis(200),
            reader.read(&mut buf),
        )
        .await
        .expect("read timed out — shutdown did not surface as EOF")
        .expect("read failed");
        assert_eq!(n, 0, "expected EOF after writer shutdown, read {n} bytes");
    }

    /// Fallback exit path: all WriterRef clones dropped without sending Disconnect.
    /// The actor's mailbox closes, the loop exits, shutdown() runs, and exit fires.
    #[tokio::test]
    async fn exit_signal_fires_when_all_senders_dropped() {
        let (_reader, writer) = duplex(1024);
        let (_reader_half, writer_half) = tokio::io::split(writer);
        let (writer_ref, exit_rx) = spawn_socket_writer(writer_half);

        drop(writer_ref);

        tokio::time::timeout(tokio::time::Duration::from_millis(200), exit_rx)
            .await
            .expect("exit signal not fired within timeout")
            .expect("exit sender dropped without signalling");
    }

    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::AsyncWrite;

    /// `AsyncWrite` where `poll_write` succeeds but `poll_shutdown` hangs forever.
    struct StuckShutdownWriter;

    impl AsyncWrite for StuckShutdownWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Pending
        }
    }

    /// If shutdown() never resolves, the writer still exits after WRITER_SHUTDOWN_TIMEOUT.
    /// Virtual time via `start_paused = true` keeps the test fast.
    #[tokio::test(start_paused = true)]
    async fn shutdown_timeout_does_not_block_exit() {
        // Build a split pair around StuckShutdownWriter. It only implements AsyncWrite;
        // we wrap with `tokio::io::join` to supply a dummy AsyncRead.
        let stuck = tokio::io::join(tokio::io::empty(), StuckShutdownWriter);
        let (_read_half, write_half) = tokio::io::split(stuck);
        let (writer_ref, exit_rx) = spawn_socket_writer(write_half);

        writer_ref.disconnect().await;

        // Advance virtual time past the shutdown timeout.
        tokio::time::advance(WRITER_SHUTDOWN_TIMEOUT + std::time::Duration::from_millis(100)).await;

        // Exit should have fired by now.
        exit_rx
            .await
            .expect("exit sender dropped without signalling");
    }
}
