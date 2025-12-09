//! Crate implementing the encoding (presentation) layer.
mod buffer;
mod definitions;
pub mod field_access;
pub mod field_types;

pub use buffer::{Buffer, BufferWriter};
pub use field_access::{FieldType, FieldValueError};

pub use definitions::HardCodedFixFieldDefinition;
#[cfg(feature = "fix42")]
pub use definitions::fix42;
#[cfg(feature = "fix44")]
pub use definitions::fix44;
pub use definitions::fixt11;
