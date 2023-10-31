//! Field and message definitions for all FIX application versions.
//!
//! # What is this and why is this necessary?
//!
//! FerrumFIX internals rely on [`Dictionary`](crate::Dictionary) for accessing
//! details about fields, messages and other abstract entities defined in the
//! FIX Dictionary specifications. Although this approach works quite well, it
//! can become daunting to query a [`Dictionary`](crate::Dictionary) for even
//! the most basic operation.

use crate::dict::FixDatatype;
use crate::{dict, TagU32};

#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct HardCodedFixFieldDefinition {
    pub name: &'static str,
    pub tag: u32,
    pub data_type: FixDatatype,
    pub location: dict::FieldLocation,
}

impl dict::IsFieldDefinition for HardCodedFixFieldDefinition {
    #[inline]
    fn tag(&self) -> TagU32 {
        TagU32::new(self.tag).expect("Invalid tag number 0.")
    }

    #[inline]
    fn name(&self) -> &str {
        self.name
    }

    #[inline]
    fn location(&self) -> dict::FieldLocation {
        self.location
    }
}

#[allow(dead_code, unused, warnings)]
#[rustfmt::skip]
/// Field and message definitions for FIX.4.4.
pub mod fix44 {
    include!(concat!(env!("OUT_DIR"), "/fix44.rs"));
}
