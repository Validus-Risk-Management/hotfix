//! An experimental  [Financial Information eXchange (FIX)](https://www.fixtrading.org/standards/) engine.
//!
//! HotFIX is a [FIX](https://www.fixtrading.org/standards/) engine implemented in Rust. While the ambition is to create a robust,
//! fully compliant, ergonomic and performant engine eventually, this is a large undertaking.
//!
//! The primary objective of HotFIX is to provide a functional and useful engine for initiators,
//! supporting FIX 4.4 and 5.0, as soon as possible. This has meant using existing solutions
//! where possible, prioritising functional components over performance and moving fast with
//! experimental code rather than good code at this stage.
//!
//! ### What's working already
//!
//! - [x] TCP transport
//! - [x] TLS support using `rustls`
//! - [x] Basic message encoding and decoding (FIX 4.4)
//! - [x] Persistent message stores
//!   - [x] [mongodb](https://www.mongodb.com/docs/drivers/rust/current/)
//!   - [x] [redb](https://www.redb.org/)
//! - [x] Heartbeats, logon, reconnecting sessions
//! - [x] Basic logic for sending messages
//! - [x] Basic logic for receiving messages
//! - [x] Resend flows
//!
//! Check out the [examples](https://github.com/Validus-Risk-Management/hotfix/tree/main/examples)
//! to get started.
pub mod application;
pub mod config;
pub(crate) mod error;
pub mod initiator;
pub mod message;
mod message_utils;
pub mod session;
mod session_schedule;
pub mod store;
pub mod transport;

pub use application::Application;
pub use hotfix_message::field_types;
pub use hotfix_message::message::Message;
