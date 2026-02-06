use std::io::Write;

use crate::FieldType;
use crate::HardCodedFixFieldDefinition;
use crate::builder::SOH;
use crate::encoder::Encode;
use crate::error::EncodingResult;
use crate::field_map::{Field, FieldMap};
use crate::parts::{Body, Header, Part, RepeatingGroup, Trailer};
use crate::session_fields::{BEGIN_STRING, BODY_LENGTH, CHECK_SUM, MSG_TYPE};
use hotfix_dictionary::{FieldLocation, IsFieldDefinition};

#[derive(Clone)]
pub struct Message {
    pub(crate) header: Header,
    pub(crate) body: Body,
    pub(crate) trailer: Trailer,
}

impl Message {
    pub fn new(begin_string: &str, message_type: &str) -> Self {
        let mut msg = Self {
            header: Header::default(),
            body: Body::default(),
            trailer: Trailer::default(),
        };
        msg.set(BEGIN_STRING, begin_string);
        msg.set(MSG_TYPE, message_type);

        msg
    }

    pub(crate) fn with_header(header: Header) -> Self {
        Self {
            header,
            body: Body::default(),
            trailer: Trailer::default(),
        }
    }

    pub fn encode(&mut self, config: &Config) -> EncodingResult<Vec<u8>> {
        let mut buffer = Vec::new();

        self.trailer.pop(CHECK_SUM);
        let body_length = self.header.calculate_length()
            + self.body.calculate_length()
            + self.trailer.calculate_length();
        self.set(BODY_LENGTH, format!("{body_length}").as_str());
        let check_sum_start = buffer.len();

        let starting_fields = vec![BEGIN_STRING.tag(), BODY_LENGTH.tag(), MSG_TYPE.tag()];
        self.header
            .fields
            .write(config, &mut buffer, &starting_fields)?;
        self.body.fields.write(config, &mut buffer, &[])?;
        self.trailer.fields.write(config, &mut buffer, &[])?;

        let checksum = buffer.as_slice()[check_sum_start..]
            .iter()
            .fold(0u8, |acc, &x| acc.wrapping_add(x));
        let checksum_value = format!("{checksum:03}");
        self.set(CHECK_SUM, checksum_value.as_str());
        buffer.write_all(b"10=")?;
        buffer.write_all(checksum_value.as_bytes())?;
        buffer.push(config.separator);

        Ok(buffer)
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn header_mut(&mut self) -> &mut Header {
        &mut self.header
    }

    pub fn trailer(&self) -> &Trailer {
        &self.trailer
    }

    pub fn get_group(
        &self,
        start_field: &HardCodedFixFieldDefinition,
        index: usize,
    ) -> Option<&RepeatingGroup> {
        let tag = start_field.tag();
        match start_field.location {
            FieldLocation::Header => self.header.get_group(tag, index),
            FieldLocation::Body => self.body.get_group(tag, index),
            FieldLocation::Trailer => self.trailer.get_group(tag, index),
        }
    }
}

impl Part for Message {
    fn get_field_map(&self) -> &FieldMap {
        self.body.get_field_map()
    }

    fn get_field_map_mut(&mut self) -> &mut FieldMap {
        self.body.get_field_map_mut()
    }

    fn set<'a, V>(&'a mut self, field_definition: &HardCodedFixFieldDefinition, value: V)
    where
        V: FieldType<'a>,
    {
        let field = Field::new(field_definition.tag(), value.to_bytes());

        match field_definition.location {
            FieldLocation::Header => self.header.store_field(field),
            FieldLocation::Body => self.body.store_field(field),
            FieldLocation::Trailer => self.trailer.store_field(field),
        };
    }
}

#[derive(Clone, Copy)]
pub struct Config {
    pub(crate) separator: u8,
}

impl Config {
    pub const fn with_separator(separator: u8) -> Self {
        Self { separator }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self { separator: SOH }
    }
}
