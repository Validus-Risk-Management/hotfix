#[cfg(feature = "smartstring")]
pub(crate) use smartstring::alias::String as SmartString;
#[cfg(not(feature = "smartstring"))]
pub(crate) use std::string::String as SmartString;
