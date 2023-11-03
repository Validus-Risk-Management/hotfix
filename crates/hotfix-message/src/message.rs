use hotfix_dictionary::{Dictionary, FieldLocation, LayoutItemKind, TagU32};
use hotfix_encoding::HardCodedFixFieldDefinition;
use std::collections::HashSet;

use crate::field_map::Field;
use crate::parts::{Body, Header, Part, Trailer};

const SOH: u8 = 0x1;

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

    pub fn get(&self, field: &HardCodedFixFieldDefinition) -> Option<&[u8]> {
        let tag = TagU32::new(field.tag).unwrap();
        let f = match field.location {
            FieldLocation::Header => self.header.get(tag),
            FieldLocation::Body => self.body.get(tag),
            FieldLocation::Trailer => self.trailer.get(tag),
        };

        f.map(|value| value.data.as_slice())
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
    dict: &'a Dictionary,
    header_tags: HashSet<TagU32>,
    trailer_tags: HashSet<TagU32>,
    position: usize,
    raw_data: &'a [u8],
    config: Config,
}

impl<'a> MessageParser<'a> {
    fn new(dict: &'a Dictionary, config: Config, data: &'a [u8]) -> Self {
        Self {
            dict,
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
            let tag = field.tag.get();
            body.store_field(field);

            // check if it's the start of a group and parse the group as needed
            let field_def = self.dict.field_by_tag(tag).unwrap();
            if field_def.is_num_in_group() {
                self.parse_group()
            }

            field = self
                .next_field()
                .expect("message to not end within the body");
        }

        (body, field)
    }

    fn build_trailer(&mut self, next_field: Field) -> Trailer {
        // https://www.onixs.biz/fix-dictionary/4.4/compblock_standardtrailer.html
        let mut trailer = Trailer::default();
        let mut field = Some(next_field);
        while let Some(f) = field {
            trailer.store_field(f);
            field = self.next_field();
        }

        trailer
    }

    fn parse_group(&mut self) {}

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
    use hotfix_dictionary::Dictionary;
    use hotfix_encoding::{fix42, fix44};

    #[test]
    fn parse_simple_message() {
        let config = Config { separator: b'|' };
        let raw = b"8=FIX.4.4|9=40|35=D|49=AFUNDMGR|56=ABROKER|15=USD|59=0|10=091|";
        let dict = Dictionary::fix44();

        let message = Message::from_bytes(config, &dict, raw);

        let begin = message.get(fix44::BEGIN_STRING).unwrap();
        assert_eq!(begin, b"FIX.4.4");

        let body_length = message.get(fix44::BODY_LENGTH).unwrap();
        assert_eq!(body_length, b"40");

        let message_type = message.get(fix44::MSG_TYPE).unwrap();
        assert_eq!(message_type, b"D");

        let currency = message.get(fix44::CURRENCY).unwrap();
        assert_eq!(currency, b"USD");

        let time_in_force = message.get(fix44::TIME_IN_FORCE).unwrap();
        assert_eq!(time_in_force, b"0");

        let checksum = message.get(fix44::CHECK_SUM).unwrap();
        assert_eq!(checksum, b"091");
    }

    #[test]
    fn repeating_group_entries() {
        let config = Config { separator: b'|' };
        let raw = b"8=FIX.4.2|9=196|35=X|49=A|56=B|34=12|52=20100318-03:21:11.364|262=A|268=2|279=0|269=0|278=BID|55=EUR/USD|270=1.37215|15=EUR|271=2500000|346=1|279=0|269=1|278=OFFER|55=EUR/USD|270=1.37224|15=EUR|271=2503200|346=1|10=171|";
        let dict = Dictionary::fix42();

        let message = Message::from_bytes(config, &dict, raw);
        let begin = message.get(fix42::BEGIN_STRING).unwrap();
        assert_eq!(begin, b"FIX.4.2");

        // TODO: add logic for repeating groups and expand this test case
    }
}
