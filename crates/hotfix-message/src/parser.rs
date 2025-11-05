use crate::Part;
use crate::error::{MessageIntegrityError, ParserError, ParserResult};
use crate::field_map::Field;
use crate::field_types::CheckSum;
use crate::message::{Config, Message};
use crate::parsed_message::{GarbledReason, InvalidReason, ParsedMessage};
use crate::parser_dictionary::{GroupDef, ParserDictionary};
use crate::parts::{Body, Header, RepeatingGroup, Trailer};
use crate::tags::{BEGIN_STRING, BODY_LENGTH, CHECK_SUM, MSG_TYPE};
use hotfix_dictionary::{Dictionary, TagU32};

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

pub struct MessageParser {
    dict: Dictionary,
    parser_dict: ParserDictionary,
    config: Config,
}

struct Parser<'a> {
    position: usize,
    raw_data: &'a [u8],
    config: &'a Config,
}

impl<'a> Parser<'a> {
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

    fn parse_checksum(&self, checksum_start: usize) -> Result<Field, MessageIntegrityError> {
        if let Some((checksum, _)) = self.parse_field_at(checksum_start)
            && checksum.tag.get() == CHECK_SUM.tag
        {
            Ok(checksum)
        } else {
            Err(MessageIntegrityError::InvalidCheckSum)
        }
    }
}

impl MessageParser {
    pub fn new(dict: Dictionary, config: Config) -> anyhow::Result<Self> {
        let parser_dict: ParserDictionary = dict.clone().try_into()?;
        let parser = Self {
            dict,
            parser_dict,
            config,
        };

        Ok(parser)
    }

    pub(crate) fn build(&self, data: &[u8]) -> ParsedMessage {
        let mut parser = Parser {
            position: 0,
            raw_data: data,
            config: &self.config,
        };
        let (mut header, mut trailer) = match self.verify_integrity(&mut parser) {
            Ok((header, trailer)) => (header, trailer),
            Err(err) => return err.into(),
        };

        let next = match self.build_header(&mut header, &mut parser) {
            Ok(next_field) => next_field,
            Err(err) => {
                return parser_error_to_parsed_message(err, header);
            }
        };

        let msg_type = header.get::<&str>(MSG_TYPE).unwrap(); // we know this is valid at this point as we have already verified the integrity of the header
        let (body, next) = match self.build_body(msg_type, &mut parser, next) {
            Ok((body, field)) => (body, field),
            Err(err) => {
                return parser_error_to_parsed_message(err, header);
            }
        };

        self.build_trailer(&mut trailer, &mut parser, next);

        let msg = Message {
            header,
            body,
            trailer,
        };

        ParsedMessage::Valid(msg)
    }

    fn verify_integrity(
        &self,
        parser: &mut Parser,
    ) -> Result<(Header, Trailer), MessageIntegrityError> {
        let mut header = Header::default();

        // The first field should always be BeginString
        let begin_string_field = self.parse_begin_string(parser)?;
        header.fields.insert(begin_string_field);

        // The second field should always be BodyLength
        let body_length_field = self.parse_body_length(parser)?;
        header.fields.insert(body_length_field);

        // The BodyLength is the number of bytes between the end of the BodyLength field and the start of the last field (i.e. the checksum)
        let body_length = if let Ok(body_length) = header.get::<usize>(BODY_LENGTH) {
            let expected_length = parser.position + body_length + CHECKSUM_LENGTH;
            if parser.raw_data.len() != expected_length {
                return Err(MessageIntegrityError::InvalidBodyLength);
            }
            body_length
        } else {
            // we failed to parse body length as usize
            return Err(MessageIntegrityError::InvalidBodyLength);
        };

        // Parse the checksum (at the end of the message) and verify it matches the computed checksum
        let mut trailer = Trailer::default();
        let checksum_field = parser.parse_checksum(parser.position + body_length)?;
        trailer.fields.insert(checksum_field);

        if let Ok(checksum) = trailer.get::<CheckSum>(CHECK_SUM) {
            let computed_checksum =
                CheckSum::compute(&parser.raw_data[0..parser.position + body_length]);
            if computed_checksum != checksum {
                return Err(MessageIntegrityError::InvalidCheckSum);
            }
        }

        // The third field should be the MsgType
        let msg_type_field = self.parse_message_type(parser)?;
        header.fields.insert(msg_type_field);

        Ok((header, trailer))
    }

    fn parse_begin_string(&self, parser: &mut Parser) -> Result<Field, MessageIntegrityError> {
        if let Some(begin_string) = parser.next_field()
            && begin_string.tag.get() == BEGIN_STRING.tag
        {
            Ok(begin_string)
        } else {
            Err(MessageIntegrityError::InvalidBeginString)
        }
    }

    fn parse_body_length(&self, parser: &mut Parser) -> Result<Field, MessageIntegrityError> {
        if let Some(body_length) = parser.next_field()
            && body_length.tag.get() == BODY_LENGTH.tag
        {
            Ok(body_length)
        } else {
            Err(MessageIntegrityError::InvalidBodyLength)
        }
    }

    fn parse_message_type(&self, parser: &mut Parser) -> Result<Field, MessageIntegrityError> {
        if let Some(msg_type) = parser.next_field()
            && msg_type.tag.get() == MSG_TYPE.tag
        {
            Ok(msg_type)
        } else {
            Err(MessageIntegrityError::InvalidMsgType)
        }
    }

    fn build_header(&self, header: &mut Header, parser: &mut Parser) -> ParserResult<Field> {
        // we have already added the first 3 mandatory fields, build the rest

        loop {
            let field = parser.next_field().ok_or(ParserError::Malformed(
                "message ended within header".to_string(),
            ))?;

            if self.parser_dict.is_header_tag(field.tag) {
                header.fields.insert(field);
            } else {
                // check the message type once all other header fields have been parsed
                // we delay it until after parsing so our rejection has access to fields like the sequence number
                let msg_type = header
                    .get::<&str>(MSG_TYPE)
                    .expect("this should never fail as we've verified the integrity of the header");
                if self.dict.message_by_msgtype(msg_type).is_none() {
                    return Err(ParserError::InvalidMsgType(msg_type.to_string()));
                }

                return Ok(field);
            }
        }
    }

    fn build_body(
        &self,
        msg_type: &str,
        parser: &mut Parser,
        next_field: Field,
    ) -> ParserResult<(Body, Field)> {
        let message_def = self.parser_dict.get_message_def(msg_type)?;
        let mut body = Body::default();
        let mut field = next_field;

        while message_def.contains_tag(field.tag) {
            let tag = field.tag.get();
            body.store_field(field);

            // check if it's the start of a group and parse the group as needed
            let field_def = self.get_dict_field_by_tag(tag)?;
            match message_def.get_group(TagU32::new(tag).unwrap()) {
                Some(group_def) => {
                    let (groups, next) = self.parse_groups(parser, group_def, field_def.tag())?;
                    body.set_groups(groups);
                    field = next;
                }
                None => {
                    field = parser.next_field().ok_or(ParserError::Malformed(
                        "message ended within the body".to_string(),
                    ))?;
                }
            }
        }

        if !self.parser_dict.is_trailer_tag(field.tag) {
            return Err(ParserError::InvalidField(field.tag.get()));
        }

        Ok((body, field))
    }

    fn build_trailer(&self, trailer: &mut Trailer, parser: &mut Parser, next_field: Field) {
        let mut field = Some(next_field);
        while let Some(f) = field {
            if f.tag.get() == CHECK_SUM.tag {
                break;
            }
            trailer.store_field(f);
            field = parser.next_field();
        }
    }

    fn parse_groups(
        &self,
        parser: &mut Parser,
        group_def: &GroupDef,
        start_tag: TagU32,
    ) -> ParserResult<(Vec<RepeatingGroup>, Field)> {
        let delimiter = group_def.delimiter_tag();
        let mut groups = vec![];

        let mut field = parser.next_field().ok_or(ParserError::Malformed(
            "missing delimiter field".to_string(),
        ))?;
        if field.tag != delimiter {
            return Err(ParserError::InvalidGroupFieldOrder {
                tag: field.tag.get(),
                group_tag: group_def.number_of_entries_tag().get(),
            });
        }
        loop {
            let mut group = RepeatingGroup::new_with_tags(start_tag, delimiter);

            // we store the first field, which is the delimiter
            group.store_field(field);

            field = parser
                .next_field()
                .ok_or(ParserError::Malformed("empty group".to_string()))?;

            // we skip the first field as we've already stored the delimiter
            for field_def in group_def.fields()[1..].iter() {
                let current_tag = field.tag;
                if field_def.tag == current_tag {
                    // the next tag is the next expected field's tag in the group, store it and move on
                    group.store_field(field);
                    field = if let Some(nested_group_def) = group_def.get_nested_group(current_tag)
                    {
                        let (groups, next) =
                            self.parse_groups(parser, nested_group_def, current_tag)?;
                        group.set_groups(groups);
                        next
                    } else {
                        parser
                            .next_field()
                            .ok_or(ParserError::Malformed("incomplete group".to_string()))?
                    }
                } else if !field_def.is_required {
                    // this field isn't required in the group, so it's fine to skip it
                } else {
                    // the next field in the group is required but the next field in the message isn't it
                    let err = if group_def.contains_tag(field.tag) {
                        ParserError::InvalidGroupFieldOrder {
                            tag: field.tag.get(),
                            group_tag: group_def.number_of_entries_tag().get(),
                        }
                    } else {
                        ParserError::InvalidField(field.tag.get())
                    };
                    return Err(err);
                }
            }

            // we've checked all fields for this group,
            // it's either another group in the repeating group or the end of the repeating group
            groups.push(group);

            if !group_def.contains_tag(field.tag) {
                return Ok((groups, field));
            }
        }
    }

    fn get_dict_field_by_tag(&self, tag: u32) -> ParserResult<hotfix_dictionary::Field<'_>> {
        self.dict
            .field_by_tag(tag)
            .ok_or(ParserError::InvalidField(tag))
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
        ParserError::InvalidGroupFieldOrder { tag, group_tag } => ParsedMessage::Invalid {
            reason: InvalidReason::InvalidOrderInGroup { tag, group_tag },
            message: Message::with_header(header),
        },
        ParserError::InvalidComponent(tag) => ParsedMessage::Invalid {
            reason: InvalidReason::InvalidComponent(tag),
            message: Message::with_header(header),
        },
        ParserError::InvalidMsgType(msg_type) => ParsedMessage::Invalid {
            reason: InvalidReason::InvalidMsgType(msg_type),
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

    const CONFIG: Config = Config::with_separator(b'|');

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

        let parsed_message = Message::from_bytes(&CONFIG, &dict, raw);
        let message = parsed_message.into_message().unwrap();
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
                reason: InvalidReason::InvalidOrderInGroup {
                    tag: 385,
                    group_tag: 384
                },
                ..
            }
        ));
    }

    #[test]
    #[ignore] // TODO: making this test pass requires an overhaul of the message parsing logic
    fn test_parsing_nested_component_inside_group() {
        // an `AllocationInstruction` with `CommissionData` nested inside `AllocGrp`
        let raw = b"8=FIX.4.4|9=252|35=J|49=SELLSIDE|56=BUYSIDE|34=100|52=20251023-14:30:00|70=ALLOC001|71=0|626=1|854=0|55=AAPL|107=Apple Inc|167=CS|54=1|53=10000|60=20251023|75=20251023|381=250000|78=2|79=ACC001|661=1|80=5000|12=100|13=3|11=5|79=ACC002|661=1|80=5000|12=75|13=2|11=3.75|10=031|";
        let dict = Dictionary::fix44();

        let parsed_message = Message::from_bytes(&CONFIG, &dict, raw)
            .into_message()
            .unwrap();

        let alloc_1 = parsed_message.get_group(fix44::NO_ALLOCS, 0).unwrap();
        assert_eq!(alloc_1.get::<&str>(fix44::ALLOC_ID).unwrap(), "ALLOC001");

        let alloc_2 = parsed_message.get_group(fix44::NO_ALLOCS, 1).unwrap();
        assert_eq!(alloc_2.get::<&str>(fix44::ALLOC_ID).unwrap(), "ALLOC002");
    }
}
