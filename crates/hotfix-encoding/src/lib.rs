//! Crate implementing the encoding (presentation) layer.
mod buffer;
pub mod config;
pub mod decoder;
mod definitions;
mod error;
pub mod field_access;
pub mod field_types;
pub mod raw_decoder;
mod streaming_decoder;
mod utils;

use hotfix_derive::FieldType;
use hotfix_dictionary::{self as dict, TagU32};

use buffer::{Buffer, BufferWriter};
pub use definitions::fix44;
use field_access::FieldType;
