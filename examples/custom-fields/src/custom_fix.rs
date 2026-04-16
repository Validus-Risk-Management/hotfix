//! Field constants generated at build time from `spec/FIX44-custom.xml`.
//!
//! See `build.rs` and the README for how this is produced.

#![allow(dead_code)]
#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/custom_fix.rs"));
