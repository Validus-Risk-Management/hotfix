//! Crate implementing the encoding (presentation) layer.
mod buffer;
mod definitions;
pub mod field_access;
pub mod field_types;

pub use buffer::{Buffer, BufferWriter};
pub use field_access::{FieldType, FieldValueError};

#[cfg(feature = "fix42")]
pub use definitions::fix42;
pub use definitions::{HardCodedFixFieldDefinition, fix44};
