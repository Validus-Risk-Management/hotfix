use tokio::io::{AsyncRead, AsyncReadExt, ReadHalf};
use tokio::sync::oneshot;
use tracing::debug;

use crate::message::OutboundMessage;
use crate::message::parser::Parser;
use crate::session::InternalSessionRef;
use crate::transport::reader::ReaderRef;

pub fn spawn_socket_reader(
    reader: ReadHalf<impl AsyncRead + Send + 'static>,
    session_ref: InternalSessionRef<impl OutboundMessage>,
) -> ReaderRef {
    let (dc_sender, dc_receiver) = oneshot::channel();
    let (kill_sender, kill_receiver) = oneshot::channel();
    let actor = ReaderActor::new(reader, session_ref, dc_sender);
    tokio::spawn(run_reader(actor, kill_receiver));

    ReaderRef::new(dc_receiver, kill_sender)
}

struct ReaderActor<M, R> {
    reader: ReadHalf<R>,
    session_ref: InternalSessionRef<M>,
    dc_sender: oneshot::Sender<()>,
}

impl<M, R: AsyncRead> ReaderActor<M, R> {
    fn new(
        reader: ReadHalf<R>,
        session_ref: InternalSessionRef<M>,
        dc_sender: oneshot::Sender<()>,
    ) -> Self {
        Self {
            reader,
            session_ref,
            dc_sender,
        }
    }
}

async fn run_reader<Outbound, R>(
    mut actor: ReaderActor<Outbound, R>,
    mut kill_rx: oneshot::Receiver<()>,
) where
    Outbound: OutboundMessage,
    R: AsyncRead,
{
    let mut parser = Parser::default();
    loop {
        let mut buf = vec![];

        tokio::select! {
            result = actor.reader.read_buf(&mut buf) => match result {
                Ok(0) => {
                    let _ = actor
                        .session_ref
                        .disconnect("received EOF".to_string())
                        .await;
                    break;
                }
                Err(err) => {
                    let _ = actor.session_ref.disconnect(err.to_string()).await;
                    break;
                }
                Ok(_) => {
                    let messages = parser.parse(&buf);

                    for msg in messages {
                        if actor
                            .session_ref
                            .new_fix_message_received(msg)
                            .await
                            .is_err()
                        {
                            debug!("reader received message but session has been terminated");
                        }
                    }
                }
            },
            res = &mut kill_rx => {
                let reason = match res {
                    Ok(()) => "forced close by watchdog",
                    Err(_) => "reader handle dropped",
                };
                let _ = actor.session_ref.disconnect(reason.to_string()).await;
                break;
            }
        }
    }
    debug!("reader loop is shutting down");
    if actor.dc_sender.send(()).is_err() {
        debug!("receiver dropped before we could notify them of reader disconnecting");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::event::SessionEvent;
    use crate::session::test_utils::create_test_session_ref;
    use tokio::io::{AsyncWriteExt, duplex};

    /// Test that the reader correctly parses a valid FIX message and sends it to the session
    /// for processing.
    #[tokio::test]
    async fn test_successful_message_parsing() {
        let (mut writer, reader) = duplex(1024);
        let (reader_half, _writer_half) = tokio::io::split(reader);
        let (session_ref, mut event_receiver) = create_test_session_ref();

        // spawn the reader
        let _reader_ref = spawn_socket_reader(reader_half, session_ref);

        // write a valid FIX message
        let fix_message = b"8=FIX.4.4\x019=77\x0135=A\x0134=1\x0149=validus-fix\x0152=20230908-08:24:56.574\x0156=FXALL\x0198=0\x01108=30\x01141=Y\x0110=037\x01";
        writer.write_all(fix_message).await.unwrap();

        // assert message was received
        match tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            event_receiver.recv(),
        )
        .await
        {
            Ok(Some(SessionEvent::FixMessageReceived(msg))) => {
                assert_eq!(msg.as_bytes(), fix_message);
            }
            Ok(other) => panic!("Expected FixMessageReceived event, got {:?}", other),
            Err(_) => panic!("Timeout waiting for message"),
        }
    }

    /// Test that the reader correctly splits the bytes into messages.
    #[tokio::test]
    async fn test_multiple_messages_in_single_read() {
        let (mut writer, reader) = duplex(1024);
        let (reader_half, _writer_half) = tokio::io::split(reader);

        let (session_ref, mut event_receiver) = create_test_session_ref();
        let _reader_ref = spawn_socket_reader(reader_half, session_ref);

        // write two messages in one go
        let msg1 = b"8=FIX.4.4\x019=77\x0135=A\x0134=1\x0149=validus-fix\x0152=20230908-08:24:56.574\x0156=FXALL\x0198=0\x01108=30\x01141=Y\x0110=037\x01";
        let msg2 = b"8=FIX.4.4\x019=77\x0135=A\x0134=2\x0149=validus-fix\x0152=20230908-08:24:58.574\x0156=FXALL\x0198=0\x01108=30\x01141=Y\x0110=040\x01";

        let mut combined = Vec::new();
        combined.extend_from_slice(msg1);
        combined.extend_from_slice(msg2);
        writer.write_all(&combined).await.unwrap();

        // verify both messages were received as individual messages
        for expected in [msg1, msg2] {
            match tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                event_receiver.recv(),
            )
            .await
            {
                Ok(Some(SessionEvent::FixMessageReceived(msg))) => {
                    assert_eq!(msg.as_bytes(), expected);
                }
                Ok(other) => panic!("Expected FixMessageReceived event, got {:?}", other),
                Err(_) => panic!("Timeout waiting for message"),
            }
        }
    }

    /// Test that the reader correctly handles messages that are split across multiple reads.
    #[tokio::test]
    async fn test_partial_message_handling() {
        let (mut writer, reader) = duplex(1024);
        let (reader_half, _writer_half) = tokio::io::split(reader);

        let (session_ref, mut event_receiver) = create_test_session_ref();
        let _reader_ref = spawn_socket_reader(reader_half, session_ref);

        // write partial message
        let partial1 = b"8=FIX.4.4\x019=77\x0135=A\x0134=1\x0149=validus-fix\x0152=20230908-08:24:56.574\x0156=FXALL";
        writer.write_all(partial1).await.unwrap();

        // should have no complete messages yet (timeout should occur)
        let result = tokio::time::timeout(
            tokio::time::Duration::from_millis(50),
            event_receiver.recv(),
        )
        .await;
        assert!(
            result.is_err(),
            "Should timeout waiting for incomplete message"
        );

        // complete the message
        let partial2 = b"\x0198=0\x01108=30\x01141=Y\x0110=037\x01";
        writer.write_all(partial2).await.unwrap();

        // now session should receive the complete message
        let mut full_message = Vec::new();
        full_message.extend_from_slice(partial1);
        full_message.extend_from_slice(partial2);
        match tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            event_receiver.recv(),
        )
        .await
        {
            Ok(Some(SessionEvent::FixMessageReceived(msg))) => {
                assert_eq!(msg.as_bytes(), &full_message[..]);
            }
            Ok(other) => panic!("Expected FixMessageReceived event, got {:?}", other),
            Err(_) => panic!("Timeout waiting for complete message"),
        }
    }

    /// Test that EOF triggers a disconnect event.
    #[tokio::test]
    async fn test_eof_triggers_disconnect() {
        let (writer, reader) = duplex(1024);
        let (reader_half, _writer_half) = tokio::io::split(reader);

        let (session_ref, mut event_receiver) = create_test_session_ref();
        let reader_ref = spawn_socket_reader(reader_half, session_ref);

        // close the writer to trigger EOF
        drop(writer);

        // verify disconnect event was sent
        match tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            event_receiver.recv(),
        )
        .await
        {
            Ok(Some(SessionEvent::Disconnected(reason))) => {
                assert_eq!(reason, "received EOF");
            }
            Ok(other) => panic!("Expected Disconnected event, got {:?}", other),
            Err(_) => panic!("Timeout waiting for disconnect event"),
        }

        // wait for disconnect signal
        let _ = reader_ref.wait_for_disconnect().await;
    }

    /// Kill signal terminates the reader even when the peer is silent, and
    /// the session observes the watchdog-sourced disconnect reason.
    #[tokio::test]
    async fn kill_signal_terminates_reader() {
        let (_writer, reader) = duplex(1024);
        let (reader_half, _writer_half) = tokio::io::split(reader);

        let (session_ref, mut event_receiver) = create_test_session_ref();
        let reader_ref = spawn_socket_reader(reader_half, session_ref);

        // Destructure so we can both fire the kill and later await the disconnect signal.
        let ReaderRef {
            disconnect_signal,
            kill,
        } = reader_ref;

        kill.send(()).expect("kill receiver dropped");

        // Reader should publish the watchdog reason to the session.
        match tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            event_receiver.recv(),
        )
        .await
        {
            Ok(Some(SessionEvent::Disconnected(reason))) => {
                assert_eq!(reason, "forced close by watchdog");
            }
            other => panic!("expected Disconnected(\"forced close by watchdog\"), got {other:?}"),
        }

        // And the disconnect signal should fire shortly after.
        tokio::time::timeout(tokio::time::Duration::from_millis(100), disconnect_signal)
            .await
            .expect("disconnect signal not fired within timeout")
            .expect("disconnect sender dropped without signalling");
    }
}
