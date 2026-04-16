mod application;
mod custom_fix;
mod messages;

use std::sync::Arc;

use tokio::sync::{Notify, mpsc};

use crate::application::TestApplication;

#[tokio::main]
async fn main() {
    let logon_signal = Arc::new(Notify::new());
    let (exec_tx, _exec_rx) = mpsc::unbounded_channel();

    let _app = TestApplication {
        logon_signal: logon_signal.clone(),
        exec_tx,
    };

    println!("custom-fields example: application wired up");
}
