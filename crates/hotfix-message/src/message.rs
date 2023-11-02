use hotfix_dictionary::{Dictionary, LayoutItem, LayoutItemKind, TagU32};
use std::collections::HashSet;

use crate::field_map::{Field, FieldMap};

const SOH: u8 = 0x1;

#[derive(Default)]
struct Header {
    pub fields: FieldMap,
}

#[derive(Default)]
struct Trailer {
    fields: FieldMap,
}

#[derive(Default)]
struct Message {
    header: Header,
    trailer: Trailer,
}

impl Message {
    fn from_bytes(config: Config, dict: &Dictionary, data: &[u8]) -> Self {
        let mut builder = MessageBuilder {
            dict,
            position: 0,
            raw_data: data,
            config,
        };

        builder.build()
    }
}

struct Config {
    separator: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self { separator: SOH }
    }
}

struct MessageBuilder<'a> {
    dict: &'a Dictionary,
    position: usize,
    raw_data: &'a [u8],
    config: Config,
}

impl<'a> MessageBuilder<'a> {
    fn build(&mut self) -> Message {
        let mut message = Message::default();
        let header = self.build_header();

        message.header = header;
        message
    }

    fn build_header(&mut self) -> Header {
        // first three fields need to be BeginString (8), BodyLength (9), and MsgType(35)
        // https://www.onixs.biz/fix-dictionary/4.4/compblock_standardheader.html
        let mut header = Header::default();

        let header_tags = self.get_tags_for_component("StandardHeader");

        loop {
            let field = self.next_field();

            if header_tags.contains(&field.tag) {
                header.fields.insert(field);
            } else {
                break;
            }
        }

        header
    }

    fn next_field(&mut self) -> Field {
        let mut iter = self.raw_data[self.position..].iter();
        let equal_sign_position = self.position + iter.position(|c| *c == b'=').unwrap();
        let bytes_until_separator = iter.position(|c| *c == self.config.separator).unwrap();
        let separator_position = equal_sign_position + bytes_until_separator + 1;

        let field = Field {
            tag: tag_from_bytes(&self.raw_data[self.position..equal_sign_position]).unwrap(),
            data: self.raw_data[equal_sign_position + 1..separator_position].to_vec(),
        };
        self.position = separator_position + 1;

        field
    }

    fn get_tags_for_component(&self, component_name: &str) -> HashSet<TagU32> {
        let mut tags = HashSet::new();
        let component = self.dict.component_by_name(component_name).unwrap();
        for item in component.items() {
            if let LayoutItemKind::Field(field) = item.kind() {
                tags.insert(field.tag());
            }
        }

        tags
    }
}

fn tag_from_bytes(bytes: &[u8]) -> Option<TagU32> {
    let mut tag = 0u32;
    for byte in bytes.iter().copied() {
        tag = tag * 10 + (byte as u32 - b'0' as u32);
    }

    TagU32::new(tag)
}

#[cfg(test)]
mod tests {
    use crate::message::{Config, Message};
    use hotfix_dictionary::{Dictionary, LayoutItemKind, TagU32};

    #[test]
    fn parse_simple_message() {
        let config = Config { separator: b'|' };
        let raw = b"8=FIX.4.2|9=40|35=D|49=AFUNDMGR|56=ABROKER|15=USD|59=0|10=091|";
        let dict = Dictionary::fix44();

        let header_comp = dict.component_by_name("StandardHeader").unwrap();
        let mut tags = vec![];
        for item in header_comp.items() {
            match item.kind() {
                LayoutItemKind::Component(_) => {}
                LayoutItemKind::Group(_, _) => {}
                LayoutItemKind::Field(field) => tags.push(field.tag()),
            }
        }
        println!("tags = {}", tags.len());

        let message = Message::from_bytes(config, &dict, raw);

        let header_fields = message.header.fields;

        let field = header_fields.get(TagU32::new(8).unwrap()).unwrap();
        assert_eq!(field.data, b"FIX.4.2");

        let field = header_fields.get(TagU32::new(9).unwrap()).unwrap();
        assert_eq!(field.data, b"40");
    }
}
