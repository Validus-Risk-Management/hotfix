use crate::encoder::Encode;
use crate::field_map::Field;
use crate::parser::{MessageParser, SOH};
use crate::parts::{Body, Header, Part, RepeatingGroup, Trailer};
use hotfix_dictionary::{Dictionary, FieldLocation, TagU32};
use hotfix_encoding::field_access::FieldType;
use hotfix_encoding::{fix44, HardCodedFixFieldDefinition};

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
        msg.set(fix44::BEGIN_STRING, begin_string);
        msg.set(fix44::MSG_TYPE, message_type);

        msg
    }

    pub fn from_bytes(config: Config, dict: &Dictionary, data: &[u8]) -> Self {
        let mut builder = MessageParser::new(dict, config, data);

        builder.build()
    }

    pub fn encode(&mut self, config: &Config) -> Vec<u8> {
        let mut buffer = Vec::new();

        let body_length = self.header.calculate_length()
            + self.body.calculate_length()
            + self.trailer.calculate_length();
        self.set(fix44::BODY_LENGTH, format!("{}", body_length).as_str());
        // TODO: this should be the actual computed checksum
        self.set(fix44::CHECK_SUM, b"100");

        self.header.fields.write(config, &mut buffer);
        self.body.fields.write(config, &mut buffer);
        self.trailer.fields.write(config, &mut buffer);

        buffer
    }

    pub fn get(&self, field: &HardCodedFixFieldDefinition) -> Option<&[u8]> {
        let tag = TagU32::new(field.tag).unwrap();
        let f = match field.location {
            FieldLocation::Header => self.header.get(tag),
            FieldLocation::Body => self.body.get(tag),
            FieldLocation::Trailer => self.trailer.get(tag),
        };

        f.map(|value| value.data.as_slice())
    }

    pub fn get_group(
        &self,
        start_field: &HardCodedFixFieldDefinition,
        index: usize,
    ) -> Option<&RepeatingGroup> {
        let tag = TagU32::new(start_field.tag).unwrap();
        match start_field.location {
            FieldLocation::Header => self.header.get_group(tag, index),
            FieldLocation::Body => self.body.get_group(tag, index),
            FieldLocation::Trailer => self.trailer.get_group(tag, index),
        }
    }

    pub fn set<'a, V>(&'a mut self, field_definition: &HardCodedFixFieldDefinition, value: V)
    where
        V: FieldType<'a>,
    {
        let tag = TagU32::new(field_definition.tag).unwrap();
        let field = Field::new(tag, value.to_bytes());

        match field_definition.location {
            FieldLocation::Header => self.header.store_field(field),
            FieldLocation::Body => self.body.store_field(field),
            FieldLocation::Trailer => self.trailer.store_field(field),
        };
    }
}

pub struct Config {
    pub(crate) separator: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self { separator: SOH }
    }
}
