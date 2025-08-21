use crate::Part;
use crate::error::{MessageIntegrityError, ParserError, ParserResult};
use crate::field_map::Field;
use crate::field_types::CheckSum;
use crate::message::{Config, Message};
use crate::parsed_message::{GarbledReason, InvalidReason, ParsedMessage};
use crate::parts::{Body, Header, RepeatingGroup, Trailer};
use crate::tags::{BEGIN_STRING, BODY_LENGTH, CHECK_SUM, MSG_TYPE};
use hotfix_dictionary::{Dictionary, LayoutItem, LayoutItemKind, TagU32};
use std::collections::{HashMap, HashSet};

pub const SOH: u8 = 0x1;

/// Length of the checksum field.
///
/// It should always be 7 bytes:
/// - 2 bytes for the tag (`10`)
/// - a byte for the separator
/// - 3 bytes for the value
/// - a byte for the final delimiter
///
/// e.g. `10=643|`
const CHECKSUM_LENGTH: usize = 7;

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

    pub(crate) fn build(mut self) -> ParsedMessage {
        let (mut header, mut trailer) = match self.verify_integrity() {
            Ok((header, trailer)) => (header, trailer),
            Err(err) => return err.into(),
        };
        let next = match self.build_header(&mut header) {
            Ok(next_field) => next_field,
            Err(err) => {
                return parser_error_to_parsed_message(err, header);
            }
        };

        let (body, next) = match self.build_body(next) {
            Ok((body, field)) => (body, field),
            Err(err) => {
                return parser_error_to_parsed_message(err, header);
            }
        };

        self.build_trailer(&mut trailer, next);

        let msg = Message {
            header,
            body,
            trailer,
        };
        ParsedMessage::Valid(msg)
    }

    fn verify_integrity(&mut self) -> Result<(Header, Trailer), MessageIntegrityError> {
        let mut header = Header::default();

        // The first field should always be BeginString
        let begin_string_field = self.parse_begin_string()?;
        header.fields.insert(begin_string_field);

        // The second field should always be BodyLength
        let body_length_field = self.parse_body_length()?;
        header.fields.insert(body_length_field);

        // The BodyLength is the number of bytes between the end of the BodyLength field and the start of the last field (i.e. the checksum)
        let body_length = if let Ok(body_length) = header.get::<usize>(BODY_LENGTH) {
            let expected_length = self.position + body_length + CHECKSUM_LENGTH;
            if self.raw_data.len() != expected_length {
                return Err(MessageIntegrityError::InvalidBodyLength);
            }
            body_length
        } else {
            // we failed to parse body length as usize
            return Err(MessageIntegrityError::InvalidBodyLength);
        };

        // Parse the checksum (at the end of the message) and verify it matches the computed checksum
        let mut trailer = Trailer::default();
        let checksum_field = self.parse_checksum(self.position + body_length)?;
        trailer.fields.insert(checksum_field);

        if let Ok(checksum) = trailer.get::<CheckSum>(CHECK_SUM) {
            let computed_checksum =
                CheckSum::compute(&self.raw_data[0..self.position + body_length]);
            if computed_checksum != checksum {
                return Err(MessageIntegrityError::InvalidCheckSum);
            }
        }

        // The third field should be the MsgType
        let msg_type_field = self.parse_message_type()?;
        header.fields.insert(msg_type_field);

        Ok((header, trailer))
    }

    fn parse_begin_string(&mut self) -> Result<Field, MessageIntegrityError> {
        if let Some(begin_string) = self.next_field()
            && begin_string.tag.get() == BEGIN_STRING.tag
        {
            Ok(begin_string)
        } else {
            Err(MessageIntegrityError::InvalidBeginString)
        }
    }

    fn parse_body_length(&mut self) -> Result<Field, MessageIntegrityError> {
        if let Some(body_length) = self.next_field()
            && body_length.tag.get() == BODY_LENGTH.tag
        {
            Ok(body_length)
        } else {
            Err(MessageIntegrityError::InvalidBodyLength)
        }
    }

    fn parse_message_type(&mut self) -> Result<Field, MessageIntegrityError> {
        if let Some(msg_type) = self.next_field()
            && msg_type.tag.get() == MSG_TYPE.tag
        {
            Ok(msg_type)
        } else {
            Err(MessageIntegrityError::InvalidMsgType)
        }
    }

    fn parse_checksum(&self, checksum_start: usize) -> Result<Field, MessageIntegrityError> {
        if let Some((checksum, _)) = self.parse_field_at(checksum_start)
            && checksum.tag.get() == CHECK_SUM.tag
        {
            Ok(checksum)
        } else {
            Err(MessageIntegrityError::InvalidCheckSum)
        }
    }

    fn build_header(&mut self, header: &mut Header) -> ParserResult<Field> {
        // we have already added the first 3 mandatory fields, build the rest
        loop {
            let field = self.next_field().ok_or(ParserError::Malformed(
                "message ended within header".to_string(),
            ))?;

            if self.header_tags.contains(&field.tag) {
                header.fields.insert(field);
            } else {
                return Ok(field);
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

    fn build_trailer(&mut self, trailer: &mut Trailer, next_field: Field) {
        let mut field = Some(next_field);
        while let Some(f) = field {
            if f.tag.get() == CHECK_SUM.tag {
                break;
            }
            trailer.store_field(f);
            field = self.next_field();
        }
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
        let (field, end_position) = self.parse_field_at(self.position)?;
        self.position = end_position + 1;

        Some(field)
    }

    fn parse_field_at(&self, position: usize) -> Option<(Field, usize)> {
        let mut iter = self.raw_data[position..].iter();
        let equal_sign_position = position + iter.position(|c| *c == b'=')?;
        let bytes_until_separator = iter.position(|c| *c == self.config.separator)?;
        let separator_position = equal_sign_position + bytes_until_separator + 1;

        let tag = tag_from_bytes(&self.raw_data[position..equal_sign_position])?;
        let data = self.raw_data[equal_sign_position + 1..separator_position].to_vec();
        let field = Field::new(tag, data);

        Some((field, separator_position))
    }

    fn get_dict_field_by_tag(&self, tag: u32) -> ParserResult<hotfix_dictionary::Field<'_>> {
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

fn parser_error_to_parsed_message(err: ParserError, header: Header) -> ParsedMessage {
    match err {
        ParserError::IOError(_) => ParsedMessage::Garbled(GarbledReason::Malformed),
        ParserError::InvalidField(tag) => ParsedMessage::Invalid {
            reason: InvalidReason::InvalidField(tag),
            message: Message::with_header(header),
        },
        ParserError::InvalidGroup(tag) => ParsedMessage::Invalid {
            reason: InvalidReason::InvalidGroup(tag),
            message: Message::with_header(header),
        },
        ParserError::InvalidComponent(tag) => ParsedMessage::Invalid {
            reason: InvalidReason::InvalidComponent(tag),
            message: Message::with_header(header),
        },
        ParserError::Malformed(_) => ParsedMessage::Garbled(GarbledReason::Malformed),
    }
}

#[cfg(test)]
mod tests {
    use crate::field_types::Currency;
    use crate::message::{Config, Message};
    use crate::parsed_message::{GarbledReason, InvalidReason, ParsedMessage};
    use crate::{Part, fix44};
    use hotfix_dictionary::{Dictionary, IsFieldDefinition};

    const CONFIG: Config = Config { separator: b'|' };

    #[test]
    fn parse_simple_message() {
        let raw = b"8=FIX.4.4|9=40|35=D|49=AFUNDMGR|56=ABROKER|15=USD|59=0|10=093|";
        let dict = Dictionary::fix44();

        let message = Message::from_bytes(&CONFIG, &dict, raw)
            .into_message()
            .unwrap();

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
        assert_eq!(checksum, "093");
    }

    #[test]
    fn repeating_group_entries() {
        let raw = b"8=FIX.4.4|9=191|35=8|49=SENDER|56=TARGET|34=123|52=20231103-12:00:00|11=12345|17=ABC123|150=2|39=1|55=XYZ|54=1|38=200|44=10|32=100|31=10|14=100|6=10|151=100|136=2|137=100|138=EUR|139=7|137=160|138=GBP|139=7|10=140|";
        let dict = Dictionary::fix44();

        let message = Message::from_bytes(&CONFIG, &dict, raw)
            .into_message()
            .unwrap();
        let begin: &str = message.header().get(fix44::BEGIN_STRING).unwrap();
        assert_eq!(begin, "FIX.4.4");

        let fee1 = message.get_group(fix44::NO_MISC_FEES, 0).unwrap();
        let amt: f64 = fee1.get(fix44::MISC_FEE_AMT).unwrap();
        assert_eq!(amt, 100.0);

        let fee2 = message.get_group(fix44::NO_MISC_FEES, 1).unwrap();
        let fee_type: &str = fee2.get(fix44::MISC_FEE_TYPE).unwrap();
        assert_eq!(fee_type, "7");

        let checksum: &str = message.trailer().get(fix44::CHECK_SUM).unwrap();
        assert_eq!(checksum, "140");
    }

    #[test]
    fn nested_repeating_group_entries() {
        let raw = b"8=FIX.4.4|9=247|35=8|34=2|49=Broker|52=20231103-09:30:00|56=Client|11=Order12345|17=Exec12345|150=0|39=0|55=APPL|54=1|38=100|32=50|31=150.00|151=50|14=50|6=150.00|453=2|448=PARTYA|447=D|452=1|802=2|523=SUBPARTYA1|803=1|523=SUBPARTYA2|803=2|448=PARTYB|447=D|452=2|10=129|";
        let dict = Dictionary::fix44();

        let message = Message::from_bytes(&CONFIG, &dict, raw)
            .into_message()
            .unwrap();
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
        assert_eq!(checksum, "129");
    }

    #[test]
    fn test_begin_string_not_the_first_tag() {
        let raw = b"9=40|8=FIX.4.4|35=D|49=AFUNDMGR|56=ABROKER|15=USD|59=0|10=093|";
        let dict = Dictionary::fix44();

        let parsed_message = Message::from_bytes(&CONFIG, &dict, raw);
        assert!(matches!(
            parsed_message,
            ParsedMessage::Garbled(GarbledReason::InvalidBeginString)
        ));
    }

    #[test]
    fn test_body_length_not_the_second_tag() {
        let raw = b"8=FIX.4.4|49=SENDER|9=191|35=8|56=TARGET|34=123|52=20231103-12:00:00|11=12345|17=ABC123|150=2|39=1|55=XYZ|54=1|38=200|44=10|32=100|31=10|14=100|6=10|151=100|136=2|137=100|138=EUR|139=7|137=160|138=GBP|139=7|10=140|";
        let dict = Dictionary::fix44();

        let parsed_message = Message::from_bytes(&CONFIG, &dict, raw);
        assert!(matches!(
            parsed_message,
            ParsedMessage::Garbled(GarbledReason::InvalidBodyLength)
        ));
    }

    #[test]
    fn test_body_length_is_wrong() {
        let raw = b"8=FIX.4.4|9=192|35=8|49=SENDER|56=TARGET|34=123|52=20231103-12:00:00|11=12345|17=ABC123|150=2|39=1|55=XYZ|54=1|38=200|44=10|32=100|31=10|14=100|6=10|151=100|136=2|137=100|138=EUR|139=7|137=160|138=GBP|139=7|10=140|";
        let dict = Dictionary::fix44();

        let parsed_message = Message::from_bytes(&CONFIG, &dict, raw);
        assert!(matches!(
            parsed_message,
            ParsedMessage::Garbled(GarbledReason::InvalidBodyLength)
        ));
    }

    #[test]
    fn test_body_length_exceeds_message_length() {
        let raw = b"8=FIX.4.4|9=500|35=8|49=SENDER|56=TARGET|34=123|52=20231103-12:00:00|11=12345|17=ABC123|150=2|39=1|55=XYZ|54=1|38=200|44=10|32=100|31=10|14=100|6=10|151=100|136=2|137=100|138=EUR|139=7|137=160|138=GBP|139=7|10=140|";
        let dict = Dictionary::fix44();

        let parsed_message = Message::from_bytes(&CONFIG, &dict, raw);
        assert!(matches!(
            parsed_message,
            ParsedMessage::Garbled(GarbledReason::InvalidBodyLength)
        ));
    }

    #[test]
    fn test_msg_type_is_not_the_third_tag() {
        let raw = b"8=FIX.4.4|9=191|49=SENDER|35=8|56=TARGET|34=123|52=20231103-12:00:00|11=12345|17=ABC123|150=2|39=1|55=XYZ|54=1|38=200|44=10|32=100|31=10|14=100|6=10|151=100|136=2|137=100|138=EUR|139=7|137=160|138=GBP|139=7|10=140|";
        let dict = Dictionary::fix44();

        let parsed_message = Message::from_bytes(&CONFIG, &dict, raw);
        assert!(matches!(
            parsed_message,
            ParsedMessage::Garbled(GarbledReason::InvalidMsgType)
        ));
    }

    #[test]
    fn test_checksum_is_not_the_last_tag() {
        let raw = b"8=FIX.4.4|9=191|35=8|49=SENDER|56=TARGET|34=123|52=20231103-12:00:00|11=12345|17=ABC123|150=2|39=1|55=XYZ|54=1|38=200|44=10|32=100|31=10|14=100|6=10|151=100|136=2|137=100|138=EUR|139=7|137=160|138=GBP|10=140|139=7|";
        let dict = Dictionary::fix44();

        let parsed_message = Message::from_bytes(&CONFIG, &dict, raw);
        assert!(matches!(
            parsed_message,
            ParsedMessage::Garbled(GarbledReason::InvalidChecksum)
        ));
    }

    #[test]
    fn test_invalid_checksum() {
        let raw = b"8=FIX.4.4|9=40|35=D|49=AFUNDMGR|56=ABROKER|15=USD|59=0|10=000|";
        let dict = Dictionary::fix44();

        let parsed_message = Message::from_bytes(&CONFIG, &dict, raw);
        assert!(matches!(
            parsed_message,
            ParsedMessage::Garbled(GarbledReason::InvalidChecksum)
        ));
    }

    #[test]
    fn test_invalid_field_in_body() {
        let raw = b"8=FIX.4.4|9=53|35=D|49=AFUNDMGR|9999=invalid|56=ABROKER|15=USD|59=0|10=229|";
        let dict = Dictionary::fix44();

        let parsed_message = Message::from_bytes(&CONFIG, &dict, raw);
        assert!(matches!(
            parsed_message,
            ParsedMessage::Invalid {
                reason: InvalidReason::InvalidField(_),
                ..
            }
        ));
    }

    #[test]
    fn test_invalid_group_in_body() {
        // tag=384 is `NoMsgTypes`, which is supposed to have `RefMsgType` (tag=372) and `MsgDirection` (tag=385)
        // in our message, `RefMsgType` is missing
        let raw = b"8=FIX.4.4|9=75|35=A|49=SENDER|56=TARGET|34=1|52=20231103-12:00:00|98=0|108=30|384=1|385=R|10=050|";
        let dict = Dictionary::fix44();

        let parsed_message = Message::from_bytes(&CONFIG, &dict, raw);
        assert!(matches!(
            parsed_message,
            ParsedMessage::Invalid {
                reason: InvalidReason::InvalidGroup(_),
                ..
            }
        ));
    }
}
