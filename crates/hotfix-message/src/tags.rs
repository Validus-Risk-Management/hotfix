use crate::HardCodedFixFieldDefinition;
use hotfix_dictionary::{FieldLocation, FixDatatype};

pub const BEGIN_STRING: &HardCodedFixFieldDefinition = &HardCodedFixFieldDefinition {
    name: "BeginString",
    tag: 8,
    data_type: FixDatatype::String,
    location: FieldLocation::Header,
};

pub const BODY_LENGTH: &HardCodedFixFieldDefinition = &HardCodedFixFieldDefinition {
    name: "BodyLength",
    tag: 9,
    data_type: FixDatatype::Length,
    location: FieldLocation::Header,
};

pub const MSG_TYPE: &HardCodedFixFieldDefinition = &HardCodedFixFieldDefinition {
    name: "MsgType",
    tag: 35,
    data_type: FixDatatype::String,
    location: FieldLocation::Header,
};

pub const CHECK_SUM: &HardCodedFixFieldDefinition = &HardCodedFixFieldDefinition {
    name: "CheckSum",
    tag: 10,
    data_type: FixDatatype::String,
    location: FieldLocation::Trailer,
};
