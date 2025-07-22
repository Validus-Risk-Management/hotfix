use std::fmt::Debug;

use async_trait::async_trait;
use tokio::{io::AsyncWriteExt, task::JoinHandle};
use tracing::debug;

use crate::message::parser::RawFixMessage;

#[async_trait]
pub trait Actor<M: Send> {
    fn run(mut self) -> JoinHandle<()>
    where
        Self: 'static + Sized + Send,
    {
        tokio::spawn(async move {
            while let Some(msg) = self.next().await {
                if !self.handle(msg).await {
                    break;
                }
            }
            debug!("writer loop is shutting down");
        })
    }
    async fn next(&mut self) -> Option<M>;
    async fn handle(&mut self, message: M) -> bool;
}

#[async_trait]
pub trait WriterModel: Clone + Debug {
    /// Create a new writer model and Spawn Actor Counterpart
    fn new<W: AsyncWriteExt + Unpin + Send + 'static>(writer: W) -> Self;

    /// Send a RawFix Message
    async fn send_raw_message(&self, msg: RawFixMessage);

    /// Disconnect from Actor
    async fn disconnect(&self);
}
