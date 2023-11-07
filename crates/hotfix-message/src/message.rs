use crate::encoder::Encode;
use crate::parser::{MessageParser, SOH};
use crate::parts::{Body, Header, Part, RepeatingGroup, Trailer};
use hotfix_dictionary::{Dictionary, FieldLocation, TagU32};
use hotfix_encoding::HardCodedFixFieldDefinition;

pub struct Message {
    pub(crate) header: Header,
    pub(crate) body: Body,
    pub(crate) trailer: Trailer,
}

impl Message {
    pub fn from_bytes(config: Config, dict: &Dictionary, data: &[u8]) -> Self {
        let mut builder = MessageParser::new(dict, config, data);

        builder.build()
    }

    pub fn encode(&self, config: &Config) -> Vec<u8> {
        let mut buffer = Vec::new();
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
}

pub struct Config {
    pub(crate) separator: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self { separator: SOH }
    }
}
