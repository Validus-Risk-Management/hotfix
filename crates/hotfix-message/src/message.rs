use hotfix_dictionary::{Dictionary, LayoutItemKind, TagU32};
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
struct Body {
    fields: FieldMap,
}

pub struct Message {
    header: Header,
    body: Body,
    trailer: Trailer,
}

impl Message {
    pub fn from_bytes(config: Config, dict: &Dictionary, data: &[u8]) -> Self {
        let mut builder = MessageParser::new(dict, config, data);

        builder.build()
    }
}

pub struct Config {
    separator: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self { separator: SOH }
    }
}

struct MessageParser<'a> {
    header_tags: HashSet<TagU32>,
    trailer_tags: HashSet<TagU32>,
    position: usize,
    raw_data: &'a [u8],
    config: Config,
}

impl<'a> MessageParser<'a> {
    fn new(dict: &'a Dictionary, config: Config, data: &'a [u8]) -> Self {
        Self {
            position: 0,
            header_tags: Self::get_tags_for_component(dict, "StandardHeader"),
            trailer_tags: Self::get_tags_for_component(dict, "StandardTrailer"),
            raw_data: data,
            config,
        }
    }

    fn build(&mut self) -> Message {
        let (header, next) = self.build_header();
        let (body, next) = self.build_body(next);
        let trailer = self.build_trailer(next);

        Message {
            header,
            body,
            trailer,
        }
    }

    fn build_header(&mut self) -> (Header, Field) {
        // first three fields need to be BeginString (8), BodyLength (9), and MsgType(35)
        // https://www.onixs.biz/fix-dictionary/4.4/compblock_standardheader.html
        let mut header = Header::default();

        loop {
            let field = self
                .next_field()
                .expect("the message to not end within the header");

            if self.header_tags.contains(&field.tag) {
                header.fields.insert(field);
            } else {
                return (header, field);
            }
        }
    }

    fn build_body(&mut self, next_field: Field) -> (Body, Field) {
        let mut body = Body::default();
        let mut field = next_field;

        while !self.trailer_tags.contains(&field.tag) {
            body.fields.insert(field);
            field = self
                .next_field()
                .expect("message to not end within the body");
        }

        (body, field)
    }

    fn build_trailer(&mut self, next_field: Field) -> Trailer {
        let mut trailer = Trailer::default();
        let mut field = Some(next_field);
        while let Some(f) = field {
            trailer.fields.insert(f);
            field = self.next_field();
        }

        trailer
    }

    fn next_field(&mut self) -> Option<Field> {
        let mut iter = self.raw_data[self.position..].iter();
        let equal_sign_position = self.position + iter.position(|c| *c == b'=')?;
        let bytes_until_separator = iter.position(|c| *c == self.config.separator)?;
        let separator_position = equal_sign_position + bytes_until_separator + 1;

        let tag = tag_from_bytes(&self.raw_data[self.position..equal_sign_position]).unwrap();
        let data = self.raw_data[equal_sign_position + 1..separator_position].to_vec();
        let field = Field::new(tag, data);

        self.position = separator_position + 1;

        Some(field)
    }

    fn get_tags_for_component(dict: &Dictionary, component_name: &str) -> HashSet<TagU32> {
        let mut tags = HashSet::new();
        let component = dict.component_by_name(component_name).unwrap();
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
    use hotfix_dictionary::{Dictionary, TagU32};

    #[test]
    fn mess_with_comps() {
        let dict = Dictionary::fix44();

        for comp in dict.components() {
            println!("{}", comp.name());
        }
    }

    #[test]
    fn parse_simple_message() {
        let config = Config { separator: b'|' };
        let raw = b"8=FIX.4.2|9=40|35=D|49=AFUNDMGR|56=ABROKER|15=USD|59=0|10=091|";
        let dict = Dictionary::fix44();

        let message = Message::from_bytes(config, &dict, raw);
        let header_fields = message.header.fields;
        let body_fields = message.body.fields;
        let trailer_fields = message.trailer.fields;

        let field = header_fields.get(TagU32::new(8).unwrap()).unwrap();
        assert_eq!(field.data, b"FIX.4.2");

        let field = header_fields.get(TagU32::new(9).unwrap()).unwrap();
        assert_eq!(field.data, b"40");

        let field = header_fields.get(TagU32::new(35).unwrap()).unwrap();
        assert_eq!(field.data, b"D");

        let field = body_fields.get(TagU32::new(15).unwrap()).unwrap();
        assert_eq!(field.data, b"USD");

        let field = body_fields.get(TagU32::new(59).unwrap()).unwrap();
        assert_eq!(field.data, b"0");

        let field = trailer_fields.get(TagU32::new(10).unwrap()).unwrap();
        assert_eq!(field.data, b"091");
    }
}
