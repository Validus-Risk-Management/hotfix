//! A [Financial Information eXchange (FIX)](https://www.fixtrading.org/standards/) engine.
//!
//! HotFIX is a [FIX](https://www.fixtrading.org/standards/) engine implemented in Rust,
//! focused on buy-side (initiator) workflows. It fully supports FIX 4.4 and the current
//! focus is on expanding support to other FIX versions.
//!
//! ### What's working already and short-term roadmap
//!
//! - [x] Network layer including TCP transport with optional TLS support using `rustls`
//! - [x] Message encoding and decoding
//! - [x] Session-layer supporting the core flows, such as logins, resends, etc.
//! - [x] Built-in message stores (in-memory, file-system, MongoDB)
//! - [x] Code-generation for FIX fields from XML specifications
//! - [x] Web API and CLI for session monitoring and management
//! - [ ] Code-generation for complete FIX messages from XML specification
//!
//! Check out the [examples](https://github.com/Validus-Risk-Management/hotfix/tree/main/examples)
//! to get started.

#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![deny(clippy::unwrap_used)]

pub mod application;
pub mod config;
pub mod initiator;
pub mod message;
pub mod session;
mod session_schedule;
pub mod store;
pub mod transport;

pub use application::Application;
pub use hotfix_message::field_types;
pub use hotfix_message::message::Message;

#[cfg(feature = "fix44")]
pub use hotfix_message::fix44;
