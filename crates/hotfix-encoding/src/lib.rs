//! Crate implementing the encoding (presentation) layer.
mod buffer;
pub mod config;
pub mod decoder;
mod error;
pub mod field_access;
pub mod field_types;
pub mod raw_decoder;
mod streaming_decoder;
mod utils;

use buffer::{Buffer, BufferWriter};
use field_access::FieldType;
