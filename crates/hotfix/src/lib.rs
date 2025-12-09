//! A [Financial Information eXchange (FIX)](https://www.fixtrading.org/standards/) engine.
//!
//! HotFIX is a [FIX](https://www.fixtrading.org/standards/) engine implemented in Rust.
//!
//! The near-term goal of HotFIX is to provide a functional and useful engine for the buy-side (initiators),
//! reaching full support of FIX 4.4 and 5.0 workflows as soon as possible.
//!
//! ### What's working already and short-term roadmap
//!
//! - [x] Network layer including TCP transport with optional TLS support using `rustls`
//! - [x] Message encoding and decoding (FIX 4.4)
//! - [x] Session-layer supporting the core flows, such as logins, resends, etc.
//! - [x] Built-in message stores
//!   - [x] in-memory
//!   - [x] [mongodb](https://www.mongodb.com/docs/drivers/rust/current/)
//!   - [x] [redb](https://www.redb.org/)
//!   - [x] Code-generation for FIX fields from XML specifications
//!   - [ ] FIX 5.0 support
//!   - [ ] Code-generation for complete FIX messages from XML specification
//!
//! Check out the [examples](https://github.com/Validus-Risk-Management/hotfix/tree/main/examples)
//! to get started.
pub mod application;
pub mod config;
pub(crate) mod error;
pub mod initiator;
pub mod message;
pub mod message_utils;
pub mod session;
mod session_schedule;
pub mod store;
pub mod transport;

pub use application::Application;
pub use hotfix_message::field_types;
pub use hotfix_message::message::Message;

#[cfg(feature = "fix44")]
pub use hotfix_message::fix44;
