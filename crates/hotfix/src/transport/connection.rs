use std::time::Duration;

use tokio::sync::oneshot;
use tracing::warn;

use crate::transport::reader::ReaderRef;
use crate::transport::writer::WriterRef;

const FORCE_CLOSE_TIMEOUT: Duration = Duration::from_secs(10);

pub struct FixConnection {
    writer: WriterRef,
    reader: ReaderRef,
    writer_exit: oneshot::Receiver<()>,
}

impl FixConnection {
    pub fn new(writer: WriterRef, reader: ReaderRef, writer_exit: oneshot::Receiver<()>) -> Self {
        Self {
            writer,
            reader,
            writer_exit,
        }
    }

    pub fn get_writer(&self) -> WriterRef {
        self.writer.clone()
    }

    pub async fn run_until_disconnect(self) {
        let Self {
            reader,
            mut writer_exit,
            ..
        } = self;
        let ReaderRef {
            mut disconnect_signal,
            kill,
        } = reader;

        tokio::select! {
            _ = &mut disconnect_signal => return,
            _ = &mut writer_exit => {}
        }

        match tokio::time::timeout(FORCE_CLOSE_TIMEOUT, &mut disconnect_signal).await {
            Ok(_) => {}
            Err(_) => {
                warn!(
                    "reader did not observe EOF within {:?}, forcing close",
                    FORCE_CLOSE_TIMEOUT
                );
                let _ = kill.send(());
                let _ = disconnect_signal.await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::writer::WriterMessage;
    use tokio::sync::mpsc;

    /// Build a `FixConnection` and return the ends the test controls:
    /// dc_sender to fire from the "reader", writer_exit_tx to fire from the "writer",
    /// and kill_rx so the test can observe or simulate the reader being killed.
    fn test_connection() -> (
        FixConnection,
        oneshot::Sender<()>,
        oneshot::Sender<()>,
        oneshot::Receiver<()>,
    ) {
        let (dc_tx, dc_rx) = oneshot::channel::<()>();
        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        let reader_ref = ReaderRef::new(dc_rx, kill_tx);

        let (writer_mpsc_tx, _writer_mpsc_rx) = mpsc::channel::<WriterMessage>(1);
        let writer_ref = WriterRef::new(writer_mpsc_tx);

        let (writer_exit_tx, writer_exit_rx) = oneshot::channel::<()>();

        let conn = FixConnection::new(writer_ref, reader_ref, writer_exit_rx);
        (conn, dc_tx, writer_exit_tx, kill_rx)
    }

    /// Reader signals disconnect first — return immediately, kill is never sent.
    #[tokio::test(start_paused = true)]
    async fn returns_on_reader_disconnect_before_writer_exit() {
        let (conn, dc_tx, _writer_exit_tx, mut kill_rx) = test_connection();

        dc_tx.send(()).expect("dc receiver dropped");

        conn.run_until_disconnect().await;

        // Kill should not have been sent. The sender has been dropped by now
        // (scope ended inside run_until_disconnect), so try_recv returns Closed
        // rather than Empty. Either way, an Ok(()) would mean kill was sent.
        assert!(
            !matches!(kill_rx.try_recv(), Ok(())),
            "kill signal should not have been sent"
        );
    }

    /// Writer exits first, reader disconnects within the watchdog window — no kill.
    #[tokio::test(start_paused = true)]
    async fn returns_when_reader_disconnects_after_writer_exit_within_timeout() {
        let (conn, dc_tx, writer_exit_tx, mut kill_rx) = test_connection();

        writer_exit_tx
            .send(())
            .expect("writer_exit receiver dropped");

        // Fire the reader disconnect from a task that runs on the same paused clock.
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let _ = dc_tx.send(());
        });

        conn.run_until_disconnect().await;

        assert!(
            !matches!(kill_rx.try_recv(), Ok(())),
            "kill signal should not have been sent when reader disconnected within timeout"
        );
    }

    /// Writer exits first, reader stays blocked past the watchdog — kill fires,
    /// and a simulated reader fires dc once it sees the kill.
    #[tokio::test(start_paused = true)]
    async fn watchdog_fires_kill_when_reader_stuck() {
        let (conn, dc_tx, writer_exit_tx, kill_rx) = test_connection();

        writer_exit_tx
            .send(())
            .expect("writer_exit receiver dropped");

        // Stand in for the reader: when the watchdog kills us, we publish dc.
        tokio::spawn(async move {
            if kill_rx.await.is_ok() {
                let _ = dc_tx.send(());
            }
        });

        let start = tokio::time::Instant::now();
        conn.run_until_disconnect().await;
        let elapsed = start.elapsed();

        assert!(
            elapsed >= FORCE_CLOSE_TIMEOUT,
            "expected watchdog to take at least {:?}, took {:?}",
            FORCE_CLOSE_TIMEOUT,
            elapsed
        );
    }
}
