use tokio::io::{AsyncRead, AsyncReadExt, ReadHalf};
use tokio::sync::oneshot;
use tracing::debug;

use crate::message::parser::Parser;
use crate::message::FixMessage;
use crate::session::SessionRef;

#[derive(Clone, Debug)]
pub struct ReaderMessage;

pub struct ReaderRef {
    disconnect_signal: oneshot::Receiver<()>,
}

impl ReaderRef {
    pub fn new<M: FixMessage>(
        reader: ReadHalf<impl AsyncRead + Send + 'static>,
        session_ref: SessionRef<M>,
    ) -> Self {
        let (dc_sender, dc_receiver) = oneshot::channel();
        let actor = ReaderActor::new(reader, session_ref, dc_sender);
        tokio::spawn(run_reader(actor));

        Self {
            disconnect_signal: dc_receiver,
        }
    }

    pub async fn wait_for_disconnect(self) {
        self.disconnect_signal
            .await
            .expect("not to drop signal prematurely");
    }
}

struct ReaderActor<M, R> {
    reader: ReadHalf<R>,
    session_ref: SessionRef<M>,
    dc_sender: oneshot::Sender<()>,
}

impl<M, R: AsyncRead> ReaderActor<M, R> {
    fn new(
        reader: ReadHalf<R>,
        session_ref: SessionRef<M>,
        dc_sender: oneshot::Sender<()>,
    ) -> Self {
        Self {
            reader,
            session_ref,
            dc_sender,
        }
    }
}

async fn run_reader<M, R>(mut actor: ReaderActor<M, R>)
where
    M: FixMessage,
    R: AsyncRead,
{
    let mut parser = Parser::default();
    loop {
        let mut buf = vec![];

        match actor.reader.read_buf(&mut buf).await {
            Ok(0) => {
                actor
                    .session_ref
                    .disconnect("received EOF".to_string())
                    .await;
                break;
            }
            Err(err) => {
                actor.session_ref.disconnect(err.to_string()).await;
                break;
            }
            Ok(_) => {
                let messages = parser.parse(&buf);

                for msg in messages {
                    actor.session_ref.new_fix_message_received(msg).await;
                }
            }
        }
    }
    debug!("reader loop is shutting down");
    actor
        .dc_sender
        .send(())
        .expect("be able to signal disconnect");
}
