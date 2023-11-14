use crate::error::{ParserError, ParserResult};
use crate::field_map::Field;
use crate::message::{Config, Message};
use crate::parts::{Body, Header, RepeatingGroup, Trailer};
use crate::Part;
use hotfix_dictionary::{Dictionary, LayoutItem, LayoutItemKind, TagU32};
use std::collections::{HashMap, HashSet};

pub const SOH: u8 = 0x1;

pub struct MessageParser<'a> {
    dict: &'a Dictionary,
    header_tags: HashSet<TagU32>,
    trailer_tags: HashSet<TagU32>,
    group_tags: HashMap<TagU32, HashSet<TagU32>>,
    position: usize,
    raw_data: &'a [u8],
    config: &'a Config,
}

impl<'a> MessageParser<'a> {
    pub fn new(dict: &'a Dictionary, config: &'a Config, data: &'a [u8]) -> ParserResult<Self> {
        let parser = Self {
            dict,
            position: 0,
            header_tags: Self::get_tags_for_component(dict, "StandardHeader")?,
            trailer_tags: Self::get_tags_for_component(dict, "StandardTrailer")?,
            group_tags: Self::get_group_tags(dict),
            raw_data: data,
            config,
        };

        Ok(parser)
    }

    pub(crate) fn build(&mut self) -> ParserResult<Message> {
        let (header, next) = self.build_header()?;
        let (body, next) = self.build_body(next)?;
        let trailer = self.build_trailer(next);

        let msg = Message {
            header,
            body,
            trailer,
        };
        Ok(msg)
    }

    fn build_header(&mut self) -> ParserResult<(Header, Field)> {
        // first three fields need to be BeginString (8), BodyLength (9), and MsgType(35)
        // https://www.onixs.biz/fix-dictionary/4.4/compblock_standardheader.html
        let mut header = Header::default();

        loop {
            let field = self.next_field().ok_or(ParserError::Malformed(
                "message ended within header".to_string(),
            ))?;

            if self.header_tags.contains(&field.tag) {
                header.fields.insert(field);
            } else {
                return Ok((header, field));
            }
        }
    }

    fn build_body(&mut self, next_field: Field) -> ParserResult<(Body, Field)> {
        let mut body = Body::default();
        let mut field = next_field;

        while !self.trailer_tags.contains(&field.tag) {
            let tag = field.tag.get();
            body.store_field(field);

            // check if it's the start of a group and parse the group as needed
            let field_def = self.get_dict_field_by_tag(tag)?;
            if field_def.is_num_in_group() {
                let (groups, next) = self.parse_groups(field_def.tag())?;
                body.set_groups(groups);
                field = next;
            } else {
                field = self.next_field().ok_or(ParserError::Malformed(
                    "message ended within the body".to_string(),
                ))?;
            }
        }

        Ok((body, field))
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

    fn parse_groups(&mut self, start_tag: TagU32) -> ParserResult<(Vec<RepeatingGroup>, Field)> {
        let first_field = self
            .next_field()
            .ok_or(ParserError::Malformed("missing begin field".to_string()))?;
        let delimiter = first_field.tag;
        let mut groups = vec![];

        let mut field = first_field;
        loop {
            let mut group = RepeatingGroup::new_with_tags(start_tag, delimiter);

            // we store the first field, which is the delimiter
            group.store_field(field);
            field = self
                .next_field()
                .ok_or(ParserError::Malformed("empty group".to_string()))?;

            loop {
                if self
                    .group_tags
                    .get(&start_tag)
                    .ok_or(ParserError::InvalidGroup(start_tag.get()))?
                    .contains(&field.tag)
                {
                    // the next tag is still part of this group
                    if field.tag == delimiter {
                        // if the next field is the delimiter, we start a new group
                        break;
                    } else {
                        let tag = field.tag;
                        group.store_field(field);
                        let field_def = self.get_dict_field_by_tag(tag.get())?;
                        if field_def.is_num_in_group() {
                            let (groups, next) = self.parse_groups(tag)?;
                            group.set_groups(groups);
                            field = next;
                            continue;
                        }
                    }
                } else {
                    // otherwise we have finished parsing the groups
                    groups.push(group);
                    return Ok((groups, field));
                }
                field = self
                    .next_field()
                    .ok_or(ParserError::Malformed("incomplete group".to_string()))?;
            }

            groups.push(group)
        }
    }

    fn next_field(&mut self) -> Option<Field> {
        let mut iter = self.raw_data[self.position..].iter();
        let equal_sign_position = self.position + iter.position(|c| *c == b'=')?;
        let bytes_until_separator = iter.position(|c| *c == self.config.separator)?;
        let separator_position = equal_sign_position + bytes_until_separator + 1;

        let tag = tag_from_bytes(&self.raw_data[self.position..equal_sign_position])?;
        let data = self.raw_data[equal_sign_position + 1..separator_position].to_vec();
        let field = Field::new(tag, data);

        self.position = separator_position + 1;

        Some(field)
    }

    fn get_dict_field_by_tag(&self, tag: u32) -> ParserResult<hotfix_dictionary::Field> {
        self.dict
            .field_by_tag(tag)
            .ok_or(ParserError::InvalidField(tag))
    }

    fn get_tags_for_component(
        dict: &Dictionary,
        component_name: &str,
    ) -> ParserResult<HashSet<TagU32>> {
        let mut tags = HashSet::new();
        let component = dict
            .component_by_name(component_name)
            .ok_or(ParserError::InvalidComponent(component_name.to_string()))?;
        for item in component.items() {
            if let LayoutItemKind::Field(field) = item.kind() {
                tags.insert(field.tag());
            }
        }

        Ok(tags)
    }

    fn get_group_tags(dict: &Dictionary) -> HashMap<TagU32, HashSet<TagU32>> {
        let mut groups: HashMap<_, HashSet<_>> = HashMap::new();

        for component in dict.components() {
            for item in component.items() {
                if let LayoutItemKind::Group(field, items) = item.kind() {
                    let group = groups.entry(field.tag()).or_default();
                    for nested in items {
                        group.extend(Self::get_tags_for_layout_item(nested));
                    }
                }
            }
        }

        groups
    }

    fn get_tags_for_layout_item(item: LayoutItem) -> HashSet<TagU32> {
        let mut tags = HashSet::new();
        match item.kind() {
            LayoutItemKind::Component(comp) => {
                for i in comp.items() {
                    tags.extend(Self::get_tags_for_layout_item(i));
                }
            }
            LayoutItemKind::Group(f, _) => {
                tags.insert(f.tag());
            }
            LayoutItemKind::Field(f) => {
                tags.insert(f.tag());
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
    use crate::field_types::Currency;
    use crate::message::{Config, Message};
    use crate::{fix44, Part};
    use hotfix_dictionary::{Dictionary, IsFieldDefinition};

    #[test]
    fn parse_simple_message() {
        let config = Config { separator: b'|' };
        let raw = b"8=FIX.4.4|9=40|35=D|49=AFUNDMGR|56=ABROKER|15=USD|59=0|10=091|";
        let dict = Dictionary::fix44();

        let message = Message::from_bytes(&config, &dict, raw).unwrap();

        let begin: &str = message.header().get(fix44::BEGIN_STRING).unwrap();
        assert_eq!(begin, "FIX.4.4");

        let body_length: u32 = message.header().get(fix44::BODY_LENGTH).unwrap();
        assert_eq!(body_length, 40);

        let message_type: &str = message.header().get(fix44::MSG_TYPE).unwrap();
        assert_eq!(message_type, "D");

        let currency: &Currency = message.get(fix44::CURRENCY).unwrap();
        assert_eq!(currency, b"USD");

        let time_in_force: &str = message.get(fix44::TIME_IN_FORCE).unwrap();
        assert_eq!(time_in_force, "0");

        let checksum: &str = message.trailer().get(fix44::CHECK_SUM).unwrap();
        assert_eq!(checksum, "091");
    }

    #[test]
    fn repeating_group_entries() {
        let config = Config { separator: b'|' };
        let raw = b"8=FIX.4.4|9=219|35=8|49=SENDER|56=TARGET|34=123|52=20231103-12:00:00|11=12345|17=ABC123|150=2|39=1|55=XYZ|54=1|38=200|44=10|32=100|31=10|14=100|6=10|151=100|136=2|137=100|138=EUR|139=7|137=160|138=GBP|139=7|10=128|";
        let dict = Dictionary::fix44();

        let message = Message::from_bytes(&config, &dict, raw).unwrap();
        let begin: &str = message.header().get(fix44::BEGIN_STRING).unwrap();
        assert_eq!(begin, "FIX.4.4");

        let fee1 = message.get_group(fix44::NO_MISC_FEES, 0).unwrap();
        let amt: f64 = fee1.get(fix44::MISC_FEE_AMT).unwrap();
        assert_eq!(amt, 100.0);

        let fee2 = message.get_group(fix44::NO_MISC_FEES, 1).unwrap();
        let fee_type: &str = fee2.get(fix44::MISC_FEE_TYPE).unwrap();
        assert_eq!(fee_type, "7");

        let checksum: &str = message.trailer().get(fix44::CHECK_SUM).unwrap();
        assert_eq!(checksum, "128");
    }

    #[test]
    fn nested_repeating_group_entries() {
        let config = Config { separator: b'|' };
        let raw = b"8=FIX.4.4|9=000|35=8|34=2|49=Broker|52=20231103-09:30:00|56=Client|11=Order12345|17=Exec12345|150=0|39=0|55=APPL|54=1|38=100|32=50|31=150.00|151=50|14=50|6=150.00|453=2|448=PARTYA|447=D|452=1|802=2|523=SUBPARTYA1|803=1|523=SUBPARTYA2|803=2|448=PARTYB|447=D|452=2|10=111|";
        let dict = Dictionary::fix44();

        let message = Message::from_bytes(&config, &dict, raw).unwrap();
        let party_a = message.get_group(fix44::NO_PARTY_I_DS, 0).unwrap();
        let party_a_0 = party_a
            .get_group(fix44::NO_PARTY_SUB_I_DS.tag(), 0)
            .unwrap();
        let sub_id_0: &str = party_a_0.get(fix44::PARTY_SUB_ID).unwrap();
        assert_eq!(sub_id_0, "SUBPARTYA1");

        let party_b = message.get_group(fix44::NO_PARTY_I_DS, 1).unwrap();
        let party_b_id: &str = party_b.get(fix44::PARTY_ID).unwrap();
        assert_eq!(party_b_id, "PARTYB");

        let party_b_role: u32 = party_b.get(fix44::PARTY_ROLE).unwrap();
        assert_eq!(party_b_role, 2);

        let checksum: &str = message.trailer().get(fix44::CHECK_SUM).unwrap();
        assert_eq!(checksum, "111");
    }
}
