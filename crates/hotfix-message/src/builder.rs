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

const USER_DEFINED_TAG_MIN: u32 = 5000;

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

pub struct MessageBuilder {
    dict: Dictionary,
    header_tags: HashSet<TagU32>,
    trailer_tags: HashSet<TagU32>,
    message_specification: HashMap<String, MessageSpecification>,
    config: Config,
}

impl MessageBuilder {
    pub fn new(dict: Dictionary, config: Config) -> ParserResult<Self> {
        let header_tags = Self::get_tags_for_component(&dict, "StandardHeader")?;
        let trailer_tags = Self::get_tags_for_component(&dict, "StandardTrailer")?;
        let message_definitions = build_message_specifications(&dict)?;

        let parser = Self {
            dict,
            header_tags,
            trailer_tags,
            message_specification: message_definitions,
            config,
        };

        Ok(parser)
    }

    pub fn build(&self, data: &[u8]) -> ParsedMessage {
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

        #[allow(clippy::expect_used)]
        let msg_type = header.get::<&str>(MSG_TYPE).expect("we know this is valid at this point as we have already verified the integrity of the header");
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

            if self.is_header_tag(field.tag) {
                header.fields.insert(field);
            } else {
                // check the message type once all other header fields have been parsed
                // we delay it until after parsing so our rejection has access to fields like the sequence number
                #[allow(clippy::expect_used)]
                let msg_type = header
                    .get::<&str>(MSG_TYPE)
                    .expect("this never fails as we've verified the integrity of the header");
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
        let message_def = self.get_message_def(msg_type)?;
        let mut body = Body::default();
        let mut field = next_field;

        loop {
            if message_def.contains_tag(field.tag) {
                let tag = field.tag;
                body.store_field(field);

                // check if it's the start of a group and parse the group as needed
                let field_def = self.get_dict_field_by_tag(tag.get())?;
                match message_def.get_group(tag) {
                    Some(group_def) => {
                        let (groups, next) =
                            self.parse_groups(parser, group_def, field_def.tag())?;
                        Self::check_required_fields_in_groups(&groups, group_def)?;
                        body.set_groups(groups).map_err(|err| {
                            ParserError::Malformed(format!(
                                "failed to set groups for tag {}: {err}",
                                tag.get()
                            ))
                        })?;
                        field = next;
                    }
                    None => {
                        field = parser.next_field().ok_or(ParserError::Malformed(
                            "message ended within the body".to_string(),
                        ))?;
                    }
                }
            } else if self.should_accept_unknown_user_defined(field.tag) {
                body.store_field(field);
                field = parser.next_field().ok_or(ParserError::Malformed(
                    "message ended within the body".to_string(),
                ))?;
            } else {
                break;
            }
        }

        if !self.is_trailer_tag(field.tag) {
            return Err(ParserError::InvalidField(field.tag.get()));
        }

        Ok((body, field))
    }

    fn should_accept_unknown_user_defined(&self, tag: TagU32) -> bool {
        !self.config.validate_user_defined_fields
            && Self::is_user_defined(tag)
            && self.dict.field_by_tag(tag.get()).is_none()
    }

    fn is_user_defined(tag: TagU32) -> bool {
        tag.get() >= USER_DEFINED_TAG_MIN
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
        group_def: &GroupSpecification,
        start_tag: TagU32,
    ) -> ParserResult<(Vec<RepeatingGroup>, Field)> {
        let mut groups = vec![];
        let delimiter_tag = group_def.delimiter_tag();

        let mut field = parser.next_field().ok_or(ParserError::Malformed(
            "missing delimiter field".to_string(),
        ))?;

        if field.tag != delimiter_tag {
            return Err(ParserError::InvalidGroupFieldOrder {
                tag: field.tag.get(),
                group_tag: group_def.number_of_entries_tag().get(),
            });
        }

        let mut current_group: Option<RepeatingGroup> = None;
        let mut next_declared_idx: usize = 0;

        loop {
            let current_tag = field.tag;

            if current_tag == delimiter_tag {
                // delimiter starts a new group instance
                if let Some(g) = current_group.take() {
                    groups.push(g);
                }
                let mut new_group = RepeatingGroup::new_with_tags(start_tag, delimiter_tag);
                new_group.store_field(field);
                current_group = Some(new_group);
                next_declared_idx = 1;
                field = parser
                    .next_field()
                    .ok_or(ParserError::Malformed("incomplete group".to_string()))?;
            } else if group_def.contains_tag(current_tag) {
                // tag is declared in this group; enforce declaration order
                #[allow(clippy::expect_used)]
                let position = group_def
                    .fields()
                    .iter()
                    .position(|f| f.tag == current_tag)
                    .expect("contains_tag guarantees the tag is present in fields()");
                if position < next_declared_idx {
                    return Err(ParserError::InvalidGroupFieldOrder {
                        tag: current_tag.get(),
                        group_tag: group_def.number_of_entries_tag().get(),
                    });
                }
                next_declared_idx = position + 1;

                #[allow(clippy::expect_used)]
                let group = current_group
                    .as_mut()
                    .expect("the delimiter opens a group before any declared field is seen");
                group.store_field(field);

                field = if let Some(nested_group_def) = group_def.get_nested_group(current_tag) {
                    let (nested_groups, next) =
                        self.parse_groups(parser, nested_group_def, current_tag)?;
                    #[allow(clippy::expect_used)]
                    group.set_groups(nested_groups).expect(
                        "parse_groups yields a non-empty run of entries sharing one start and delimiter tag",
                    );
                    next
                } else {
                    parser
                        .next_field()
                        .ok_or(ParserError::Malformed("incomplete group".to_string()))?
                };
            } else if self.should_accept_unknown_user_defined(current_tag) {
                #[allow(clippy::expect_used)]
                let group = current_group
                    .as_mut()
                    .expect("the delimiter opens a group before any user-defined field is seen");
                group.store_field(field);
                field = parser
                    .next_field()
                    .ok_or(ParserError::Malformed("incomplete group".to_string()))?;
            } else {
                if let Some(g) = current_group.take() {
                    groups.push(g);
                }
                return Ok((groups, field));
            }
        }
    }

    fn check_required_fields_in_groups(
        groups: &[RepeatingGroup],
        group_def: &GroupSpecification,
    ) -> ParserResult<()> {
        let group_tag = group_def.number_of_entries_tag().get();
        for entry in groups {
            let field_map = entry.get_fields();

            for field_def in group_def.fields() {
                if field_def.is_required && !field_map.fields.contains_key(&field_def.tag) {
                    return Err(ParserError::RequiredFieldMissing {
                        tag: field_def.tag.get(),
                        group_tag: Some(group_tag),
                    });
                }
            }

            for (nested_tag, nested_spec) in &group_def.nested_groups {
                if let Some(nested_entries) = field_map.groups.get(nested_tag) {
                    Self::check_required_fields_in_groups(nested_entries, nested_spec)?;
                }
            }
        }
        Ok(())
    }

    fn get_dict_field_by_tag(&self, tag: u32) -> ParserResult<hotfix_dictionary::Field<'_>> {
        self.dict
            .field_by_tag(tag)
            .ok_or(ParserError::InvalidField(tag))
    }

    fn is_header_tag(&self, tag: TagU32) -> bool {
        self.header_tags.contains(&tag)
    }

    fn is_trailer_tag(&self, tag: TagU32) -> bool {
        self.trailer_tags.contains(&tag)
    }

    fn get_message_def(&self, msg_type: &str) -> ParserResult<&MessageSpecification> {
        match self.message_specification.get(msg_type) {
            Some(message_def) => Ok(message_def),
            None => Err(ParserError::InvalidMsgType(msg_type.to_string())),
        }
    }

    fn get_tags_for_component(
        dict: &Dictionary,
        component_name: &str,
    ) -> ParserResult<HashSet<TagU32>> {
        let mut tags = HashSet::new();
        let component = dict
            .component_by_name(component_name)
            .ok_or_else(|| ParserError::InvalidComponent(component_name.to_string()))?;
        for item in component.items() {
            if let LayoutItemKind::Field(field) = item.kind() {
                tags.insert(field.tag());
            }
        }

        Ok(tags)
    }
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
        ParserError::RequiredFieldMissing { tag, group_tag } => ParsedMessage::Invalid {
            reason: InvalidReason::RequiredFieldMissing { tag, group_tag },
            message: Message::with_header(header),
        },
        ParserError::Malformed(_) => ParsedMessage::Garbled(GarbledReason::Malformed),
    }
}

struct FieldSpecification {
    pub(crate) tag: TagU32,
    pub(crate) is_required: bool,
}

struct GroupSpecification {
    number_of_entries_tag: TagU32,
    fields: Vec<FieldSpecification>,
    nested_groups: HashMap<TagU32, GroupSpecification>,
}

impl GroupSpecification {
    pub fn fields(&self) -> &[FieldSpecification] {
        self.fields.as_slice()
    }
    pub fn number_of_entries_tag(&self) -> TagU32 {
        self.number_of_entries_tag
    }

    pub fn delimiter_tag(&self) -> TagU32 {
        #[allow(clippy::expect_used)]
        self.fields
            .first()
            .expect("groups always have at least one field")
            .tag
    }

    pub fn contains_tag(&self, tag: TagU32) -> bool {
        self.fields.iter().any(|f| f.tag == tag)
    }

    pub fn get_nested_group(&self, tag: TagU32) -> Option<&GroupSpecification> {
        self.nested_groups.get(&tag)
    }
}

struct MessageSpecification {
    fields: Vec<FieldSpecification>,
    groups: HashMap<TagU32, GroupSpecification>,
}

impl MessageSpecification {
    pub fn contains_tag(&self, tag: TagU32) -> bool {
        self.fields.iter().any(|f| f.tag == tag)
    }

    pub fn get_group(&self, tag: TagU32) -> Option<&GroupSpecification> {
        self.groups.get(&tag)
    }
}

fn build_message_specifications(
    dict: &Dictionary,
) -> ParserResult<HashMap<String, MessageSpecification>> {
    let mut definitions = HashMap::new();

    for message in dict.messages() {
        let fields = message
            .layout()
            .flat_map(|item| extract_fields(dict, item))
            .flatten()
            .collect();

        let mut groups = HashMap::new();
        for item in message.layout() {
            groups.extend(extract_groups(dict, item)?);
        }

        let message_def = MessageSpecification { fields, groups };
        definitions.insert(message.msg_type().to_string(), message_def);
    }

    Ok(definitions)
}

fn extract_fields(dict: &Dictionary, item: LayoutItem) -> ParserResult<Vec<FieldSpecification>> {
    let is_required = item.required();
    let fields = match item.kind() {
        LayoutItemKind::Component(c) => {
            let component = dict
                .component_by_name(c.name())
                .ok_or_else(|| ParserError::InvalidComponent(c.name().to_string()))?;
            component
                .items()
                .flat_map(|i| extract_fields(dict, i))
                .flatten()
                .collect()
        }
        LayoutItemKind::Field(field) => vec![FieldSpecification {
            tag: field.tag(),
            is_required,
        }],
        LayoutItemKind::Group(field, _) => vec![FieldSpecification {
            tag: field.tag(),
            is_required,
        }],
    };

    Ok(fields)
}

fn extract_groups(
    dict: &Dictionary,
    item: LayoutItem,
) -> ParserResult<HashMap<TagU32, GroupSpecification>> {
    let mut groups = HashMap::new();
    match item.kind() {
        LayoutItemKind::Component(c) => {
            let component = dict
                .component_by_name(c.name())
                .ok_or_else(|| ParserError::InvalidComponent(c.name().to_string()))?;
            for i in component.items() {
                groups.extend(extract_groups(dict, i)?);
            }
        }
        LayoutItemKind::Group(field, items) => {
            let mut nested_groups = HashMap::new();
            for i in items.iter() {
                nested_groups.extend(extract_groups(dict, i.clone())?);
            }
            groups.insert(
                field.tag(),
                GroupSpecification {
                    number_of_entries_tag: field.tag(),
                    fields: items
                        .iter()
                        .flat_map(|i| extract_fields(dict, i.clone()))
                        .flatten()
                        .collect(),
                    nested_groups,
                },
            );
        }
        _ => {}
    };

    Ok(groups)
}

#[cfg(test)]
mod tests {
    use crate::builder::MessageBuilder;
    use crate::field_types::Currency;
    use crate::message::Config;
    use crate::parsed_message::{GarbledReason, InvalidReason, ParsedMessage};
    use crate::{Part, fix44};
    use hotfix_dictionary::{Dictionary, IsFieldDefinition, TagU32};

    const CONFIG: Config = Config::with_separator(b'|');

    #[test]
    fn test_specification_top_level_fields() {
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let message_def = builder.get_message_def("J").unwrap();

        // check that it contains `Symbol`, a tag from the nested `Instrument` component
        assert!(message_def.contains_tag(fix44::SYMBOL.tag()));

        // check that it contains `NoOrders`, the starting tag for `OrdAllocGrp`
        assert!(message_def.contains_tag(fix44::NO_ORDERS.tag()));

        // check that it doesn't contain other tags from the `OrdAllocGroup`
        assert!(!message_def.contains_tag(fix44::ORDER_QTY.tag()));
    }

    #[test]
    fn test_specification_top_level_groups() {
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let message_def = builder.get_message_def("J").unwrap();

        // check that it contains the right number of top-level groups
        // expected 10 groups (7 directly (including `Parties` and `Stipulations`), 2 in `Instrument`, 1 in `InstrumentExtension`,
        let expected_group_fields = vec![
            fix44::NO_ORDERS,
            fix44::NO_ALLOCS,
            fix44::NO_EXECS,
            fix44::NO_STIPULATIONS,
            fix44::NO_PARTY_I_DS,
            fix44::NO_SECURITY_ALT_ID,
            fix44::NO_LEGS,
            fix44::NO_UNDERLYINGS,
            fix44::NO_EVENTS,
            fix44::NO_INSTR_ATTRIB,
        ];
        assert_eq!(message_def.groups.len(), expected_group_fields.len());
        for field in expected_group_fields {
            assert!(
                message_def
                    .get_group(TagU32::new(field.tag).unwrap())
                    .is_some()
            );
        }

        // check that nested groups are not included directly
        assert!(
            message_def
                .get_group(fix44::NO_NESTED2_PARTY_I_DS.tag())
                .is_none()
        );
    }

    #[test]
    fn test_specification_nested_groups() {
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let message_def = builder.get_message_def("J").unwrap();

        // Order allocation groups only have one nested group, the parties
        let order_alloc_group = message_def.get_group(fix44::NO_ORDERS.tag()).unwrap();
        assert_eq!(order_alloc_group.nested_groups.len(), 1);
        let nested_parties_2_group = order_alloc_group
            .get_nested_group(fix44::NO_NESTED2_PARTY_I_DS.tag())
            .expect("nested parties group to exist");

        // The parties group only has one nested group, the parties subgroup
        assert_eq!(nested_parties_2_group.nested_groups.len(), 1);
        let subgroup = nested_parties_2_group
            .get_nested_group(fix44::NO_NESTED2_PARTY_SUB_I_DS.tag())
            .expect("parties subgroup to exist");
        assert!(subgroup.nested_groups.is_empty());
    }

    #[test]
    fn test_specification_field_order_in_nested_group() {
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let message_def = builder.get_message_def("J").unwrap();

        // get the parties group nested in the order allocation group
        let order_alloc_group = message_def.get_group(fix44::NO_ORDERS.tag()).unwrap();
        assert_eq!(order_alloc_group.nested_groups.len(), 1);
        let nested_parties_2_group = order_alloc_group
            .get_nested_group(fix44::NO_NESTED2_PARTY_I_DS.tag())
            .expect("nested parties group to exist");

        let mut fields = nested_parties_2_group.fields.iter();
        let expected_fields = vec![
            (fix44::NESTED2_PARTY_ID, false),
            (fix44::NESTED2_PARTY_ID_SOURCE, false),
            (fix44::NESTED2_PARTY_ROLE, false),
            (fix44::NO_NESTED2_PARTY_SUB_I_DS, false),
        ];

        for (field_definition, is_required) in expected_fields {
            let next = fields.next().unwrap();
            assert_eq!(next.tag.get(), field_definition.tag);
            assert_eq!(next.is_required, is_required);
        }
    }

    #[test]
    fn parse_simple_message() {
        let raw = b"8=FIX.4.4|9=40|35=D|49=AFUNDMGR|56=ABROKER|15=USD|59=0|10=093|";
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();

        let message = builder.build(raw).into_message().unwrap();

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
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();

        let message = builder.build(raw).into_message().unwrap();

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
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let message = builder.build(raw).into_message().unwrap();

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
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let parsed_message = builder.build(raw);

        assert!(matches!(
            parsed_message,
            ParsedMessage::Garbled(GarbledReason::InvalidBeginString)
        ));
    }

    #[test]
    fn test_body_length_not_the_second_tag() {
        let raw = b"8=FIX.4.4|49=SENDER|9=191|35=8|56=TARGET|34=123|52=20231103-12:00:00|11=12345|17=ABC123|150=2|39=1|55=XYZ|54=1|38=200|44=10|32=100|31=10|14=100|6=10|151=100|136=2|137=100|138=EUR|139=7|137=160|138=GBP|139=7|10=140|";
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let parsed_message = builder.build(raw);

        assert!(matches!(
            parsed_message,
            ParsedMessage::Garbled(GarbledReason::InvalidBodyLength)
        ));
    }

    #[test]
    fn test_body_length_is_wrong() {
        let raw = b"8=FIX.4.4|9=192|35=8|49=SENDER|56=TARGET|34=123|52=20231103-12:00:00|11=12345|17=ABC123|150=2|39=1|55=XYZ|54=1|38=200|44=10|32=100|31=10|14=100|6=10|151=100|136=2|137=100|138=EUR|139=7|137=160|138=GBP|139=7|10=140|";
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let parsed_message = builder.build(raw);

        assert!(matches!(
            parsed_message,
            ParsedMessage::Garbled(GarbledReason::InvalidBodyLength)
        ));
    }

    #[test]
    fn test_body_length_exceeds_message_length() {
        let raw = b"8=FIX.4.4|9=500|35=8|49=SENDER|56=TARGET|34=123|52=20231103-12:00:00|11=12345|17=ABC123|150=2|39=1|55=XYZ|54=1|38=200|44=10|32=100|31=10|14=100|6=10|151=100|136=2|137=100|138=EUR|139=7|137=160|138=GBP|139=7|10=140|";
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let parsed_message = builder.build(raw);

        assert!(matches!(
            parsed_message,
            ParsedMessage::Garbled(GarbledReason::InvalidBodyLength)
        ));
    }

    #[test]
    fn test_msg_type_is_not_the_third_tag() {
        let raw = b"8=FIX.4.4|9=191|49=SENDER|35=8|56=TARGET|34=123|52=20231103-12:00:00|11=12345|17=ABC123|150=2|39=1|55=XYZ|54=1|38=200|44=10|32=100|31=10|14=100|6=10|151=100|136=2|137=100|138=EUR|139=7|137=160|138=GBP|139=7|10=140|";
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let parsed_message = builder.build(raw);

        assert!(matches!(
            parsed_message,
            ParsedMessage::Garbled(GarbledReason::InvalidMsgType)
        ));
    }

    #[test]
    fn test_checksum_is_not_the_last_tag() {
        let raw = b"8=FIX.4.4|9=191|35=8|49=SENDER|56=TARGET|34=123|52=20231103-12:00:00|11=12345|17=ABC123|150=2|39=1|55=XYZ|54=1|38=200|44=10|32=100|31=10|14=100|6=10|151=100|136=2|137=100|138=EUR|139=7|137=160|138=GBP|10=140|139=7|";
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let parsed_message = builder.build(raw);

        assert!(matches!(
            parsed_message,
            ParsedMessage::Garbled(GarbledReason::InvalidChecksum)
        ));
    }

    #[test]
    fn test_invalid_checksum() {
        let raw = b"8=FIX.4.4|9=40|35=D|49=AFUNDMGR|56=ABROKER|15=USD|59=0|10=000|";
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let parsed_message = builder.build(raw);

        assert!(matches!(
            parsed_message,
            ParsedMessage::Garbled(GarbledReason::InvalidChecksum)
        ));
    }

    #[test]
    fn test_invalid_field_in_body() {
        let raw = b"8=FIX.4.4|9=53|35=D|49=AFUNDMGR|9999=invalid|56=ABROKER|15=USD|59=0|10=229|";
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let parsed_message = builder.build(raw);

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
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let parsed_message = builder.build(raw);

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
    fn test_parsing_nested_component_inside_group() {
        // an `AllocationInstruction` with `CommissionData` nested inside `AllocGrp`
        let raw_instrument = "55=AAPL|107=Apple Inc|167=CS";
        let raw_alloc_group =
            "78=2|79=ACC001|661=1|80=5000|12=100|13=3|79=ACC002|661=1|80=5000|12=75|13=2";
        let raw = format!(
            "8=FIX.4.4|9=222|35=J|49=SELLSIDE|56=BUYSIDE|34=100|52=20251023-14:30:00|70=ALLOC001|71=0|626=1|857=0|54=1|{raw_instrument}|53=10000|6=125|75=20251023|{raw_alloc_group}|10=068|"
        );
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let message = builder.build(raw.as_bytes()).into_message().unwrap();

        let alloc_1 = message.get_group(fix44::NO_ALLOCS, 0).unwrap();
        assert_eq!(alloc_1.get::<&str>(fix44::ALLOC_ACCOUNT).unwrap(), "ACC001");
        assert_eq!(alloc_1.get::<f64>(fix44::COMMISSION).unwrap(), 100.0);
        assert_eq!(alloc_1.get::<&str>(fix44::COMM_TYPE).unwrap(), "3");

        let alloc_2 = message.get_group(fix44::NO_ALLOCS, 1).unwrap();
        assert_eq!(alloc_2.get::<&str>(fix44::ALLOC_ACCOUNT).unwrap(), "ACC002");
        assert_eq!(alloc_2.get::<f64>(fix44::COMMISSION).unwrap(), 75.0);
        assert_eq!(alloc_2.get::<&str>(fix44::COMM_TYPE).unwrap(), "2");
    }

    const CONFIG_NO_USER_VALIDATION: Config =
        Config::with_separator(b'|').validate_user_defined_fields(false);

    fn build_pipe_separated_message(content: &str) -> Vec<u8> {
        let body_length = content.len();
        let prefix = format!("8=FIX.4.4|9={body_length}|{content}");
        let checksum: u8 = prefix.bytes().fold(0u8, |acc, x| acc.wrapping_add(x));
        format!("{prefix}10={checksum:03}|").into_bytes()
    }

    #[test]
    fn test_unknown_user_defined_tag_at_body_rejected_by_default() {
        let raw =
            build_pipe_separated_message("35=D|49=SENDER|56=TARGET|55=AAPL|59=0|20000=custom|");
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let parsed = builder.build(&raw);

        assert!(matches!(
            parsed,
            ParsedMessage::Invalid {
                reason: InvalidReason::InvalidField(20000),
                ..
            }
        ));
    }

    #[test]
    fn test_unknown_user_defined_tag_at_body_accepted_when_validation_disabled() {
        let raw =
            build_pipe_separated_message("35=D|49=SENDER|56=TARGET|55=AAPL|59=0|20000=custom|");
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG_NO_USER_VALIDATION).unwrap();
        let message = builder.build(&raw).into_message().unwrap();

        let custom_tag = TagU32::new(20000).unwrap();
        let custom_value = message.get_field_map().get_raw(custom_tag).unwrap();
        assert_eq!(custom_value, b"custom");

        // Known fields should still be reachable.
        assert_eq!(message.get::<&str>(fix44::SYMBOL).unwrap(), "AAPL");
    }

    #[test]
    fn test_unknown_user_defined_tag_inside_group_stays_in_group_when_validation_disabled() {
        let raw = build_pipe_separated_message(
            "35=D|49=SENDER|56=TARGET|453=1|448=PARTYA|447=D|452=1|20000=custom|55=AAPL|",
        );
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG_NO_USER_VALIDATION).unwrap();
        let message = builder.build(&raw).into_message().unwrap();

        // 20000 doesn't belong to the parent message and isn't a
        // trailer field, so with user-defined validation disabled it stays
        // inside the current group rather than leaking to the body.
        let party = message.get_group(fix44::NO_PARTY_I_DS, 0).unwrap();
        assert_eq!(party.get::<&str>(fix44::PARTY_ID).unwrap(), "PARTYA");
        assert!(message.get_group(fix44::NO_PARTY_I_DS, 1).is_none());

        let custom_tag = TagU32::new(20000).unwrap();
        let in_group = party.get_field_map().get_raw(custom_tag).unwrap();
        assert_eq!(in_group, b"custom");

        // The body must NOT have picked up the unknown tag.
        assert!(message.get_field_map().get_raw(custom_tag).is_none());

        // Tag 55 belongs to NewOrderSingle, so it ends the group and lands on the body.
        assert_eq!(message.get::<&str>(fix44::SYMBOL).unwrap(), "AAPL");
    }

    #[test]
    fn test_unknown_user_defined_tag_inside_group_rejected_when_validation_enabled() {
        let raw = build_pipe_separated_message(
            "35=D|49=SENDER|56=TARGET|453=1|448=PARTYA|447=D|452=1|20000=custom|55=AAPL|",
        );
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let parsed = builder.build(&raw);

        assert!(matches!(
            parsed,
            ParsedMessage::Invalid {
                reason: InvalidReason::InvalidField(20000),
                ..
            }
        ));
    }

    #[test]
    fn test_unknown_user_defined_tag_inside_nested_group_stays_in_nested_group() {
        // 20000 appears inside a NoPartySubIDs entry. NoPartySubIDs is the
        // immediate parent (a Group) and doesn't contain 20000, so the field
        // is kept inside the nested entry. The trailer (10) ends both the
        // nested group and the outer NoPartyIDs group cleanly.
        let raw = build_pipe_separated_message(
            "35=D|49=SENDER|56=TARGET|453=1|448=PARTYA|447=D|452=1|802=1|523=SUB1|803=1|20000=custom|",
        );
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG_NO_USER_VALIDATION).unwrap();
        let message = builder.build(&raw).into_message().unwrap();

        let party = message.get_group(fix44::NO_PARTY_I_DS, 0).unwrap();
        let sub = party.get_group(fix44::NO_PARTY_SUB_I_DS.tag(), 0).unwrap();
        assert_eq!(sub.get::<&str>(fix44::PARTY_SUB_ID).unwrap(), "SUB1");

        let custom_tag = TagU32::new(20000).unwrap();
        // 20000 lives inside the nested NoPartySubIDs entry...
        let in_sub = sub.get_field_map().get_raw(custom_tag).unwrap();
        assert_eq!(in_sub, b"custom");
        // ...not on the outer NoPartyIDs entry...
        assert!(party.get_field_map().get_raw(custom_tag).is_none());
        // ...nor on the body.
        assert!(message.get_field_map().get_raw(custom_tag).is_none());
    }

    #[test]
    fn test_group_rejects_when_first_tag_is_not_delimiter() {
        // 453 (NoPartyIDs) declares the count but the first tag after it is
        // 20000 instead of the delimiter 448. This is a
        // hard reject regardless of user-defined-field validation.
        let raw = build_pipe_separated_message(
            "35=D|49=SENDER|56=TARGET|453=1|20000=x|448=PARTYA|447=D|452=1|",
        );
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG_NO_USER_VALIDATION).unwrap();
        let parsed = builder.build(&raw);

        assert!(matches!(
            parsed,
            ParsedMessage::Invalid {
                reason: InvalidReason::InvalidOrderInGroup {
                    tag: 20000,
                    group_tag: 453,
                },
                ..
            }
        ));
    }

    #[test]
    fn test_known_outer_field_after_group_fields_ends_group() {
        let raw = build_pipe_separated_message(
            "35=D|49=SENDER|56=TARGET|453=1|448=PARTYA|447=D|452=1|55=AAPL|59=0|",
        );
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let message = builder.build(&raw).into_message().unwrap();

        let party = message.get_group(fix44::NO_PARTY_I_DS, 0).unwrap();
        assert_eq!(party.get::<&str>(fix44::PARTY_ID).unwrap(), "PARTYA");
        assert!(message.get_group(fix44::NO_PARTY_I_DS, 1).is_none());

        // Symbol must not have been pulled into the group.
        assert!(party.get_raw(fix44::SYMBOL).is_none());
        assert_eq!(message.get::<&str>(fix44::SYMBOL).unwrap(), "AAPL");
        assert_eq!(message.get::<&str>(fix44::TIME_IN_FORCE).unwrap(), "0");
    }

    #[test]
    fn test_nested_and_outer_group_end_on_body_field() {
        let raw = build_pipe_separated_message(
            "35=D|49=SENDER|56=TARGET|453=1|448=PARTYA|447=D|452=1|802=1|523=SUB1|803=1|55=AAPL|59=0|",
        );
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let message = builder.build(&raw).into_message().unwrap();

        let party = message.get_group(fix44::NO_PARTY_I_DS, 0).unwrap();
        let sub = party.get_group(fix44::NO_PARTY_SUB_I_DS.tag(), 0).unwrap();
        assert_eq!(sub.get::<&str>(fix44::PARTY_SUB_ID).unwrap(), "SUB1");

        assert_eq!(message.get::<&str>(fix44::SYMBOL).unwrap(), "AAPL");
        assert_eq!(message.get::<&str>(fix44::TIME_IN_FORCE).unwrap(), "0");
    }

    #[test]
    fn test_declared_group_field_out_of_order_is_rejected() {
        let raw =
            build_pipe_separated_message("35=D|49=SENDER|56=TARGET|453=1|448=PARTYA|452=1|447=D|");
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let parsed = builder.build(&raw);

        assert!(matches!(
            parsed,
            ParsedMessage::Invalid {
                reason: InvalidReason::InvalidOrderInGroup {
                    tag: 447,
                    group_tag: 453,
                },
                ..
            }
        ));
    }

    #[test]
    fn test_multiple_nested_group_instances_are_parsed() {
        let raw = build_pipe_separated_message(
            "35=D|49=SENDER|56=TARGET|453=1|448=PARTYA|447=D|452=1|802=2|523=SUB1|803=1|523=SUB2|803=2|55=AAPL|",
        );
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG).unwrap();
        let message = builder.build(&raw).into_message().unwrap();

        let party = message.get_group(fix44::NO_PARTY_I_DS, 0).unwrap();
        let sub0 = party.get_group(fix44::NO_PARTY_SUB_I_DS.tag(), 0).unwrap();
        let sub1 = party.get_group(fix44::NO_PARTY_SUB_I_DS.tag(), 1).unwrap();
        assert_eq!(sub0.get::<&str>(fix44::PARTY_SUB_ID).unwrap(), "SUB1");
        assert_eq!(sub1.get::<&str>(fix44::PARTY_SUB_ID).unwrap(), "SUB2");
        assert!(party.get_group(fix44::NO_PARTY_SUB_I_DS.tag(), 2).is_none());

        assert_eq!(message.get::<&str>(fix44::SYMBOL).unwrap(), "AAPL");
    }

    const NESTED_REQUIRED_SPEC: &str = r#"<fix type='FIX' major='4' minor='4' servicepack='0'>
 <header>
  <field name='BeginString' required='Y'/>
  <field name='BodyLength' required='Y'/>
  <field name='MsgType' required='Y'/>
  <field name='SenderCompID' required='Y'/>
  <field name='TargetCompID' required='Y'/>
  <field name='MsgSeqNum' required='Y'/>
  <field name='SendingTime' required='Y'/>
 </header>
 <trailer>
  <field name='CheckSum' required='Y'/>
 </trailer>
 <messages>
  <message name='NestedReqMsg' msgtype='U1' msgcat='app'>
   <group name='NoOuter' required='N'>
    <field name='OuterDelim' required='N'/>
    <group name='NoInner' required='N'>
     <field name='InnerDelim' required='N'/>
     <field name='InnerRequired' required='Y'/>
    </group>
   </group>
   <field name='CustomBody' required='N'/>
  </message>
 </messages>
 <components>
 </components>
 <fields>
  <field number='8' name='BeginString' type='STRING'/>
  <field number='9' name='BodyLength' type='LENGTH'/>
  <field number='35' name='MsgType' type='STRING'/>
  <field number='49' name='SenderCompID' type='STRING'/>
  <field number='56' name='TargetCompID' type='STRING'/>
  <field number='34' name='MsgSeqNum' type='SEQNUM'/>
  <field number='52' name='SendingTime' type='UTCTIMESTAMP'/>
  <field number='10' name='CheckSum' type='STRING'/>
  <field number='6001' name='NoOuter' type='NUMINGROUP'/>
  <field number='6002' name='OuterDelim' type='STRING'/>
  <field number='6003' name='NoInner' type='NUMINGROUP'/>
  <field number='6004' name='InnerDelim' type='STRING'/>
  <field number='6005' name='InnerRequired' type='STRING'/>
  <field number='5100' name='CustomBody' type='STRING'/>
 </fields>
</fix>"#;

    #[test]
    fn test_declared_user_defined_body_field_ends_group_when_validation_disabled() {
        let dict = Dictionary::from_quickfix_spec(NESTED_REQUIRED_SPEC).unwrap();
        let raw = build_pipe_separated_message(
            "35=U1|49=S|56=T|34=1|52=20231103-12:00:00|6001=1|6002=od|6003=1|6004=id|6005=req|5100=v|",
        );
        let builder = MessageBuilder::new(dict, CONFIG_NO_USER_VALIDATION).unwrap();
        let message = builder.build(&raw).into_message().unwrap();

        let custom_tag = TagU32::new(5100).unwrap();
        let outer_tag = TagU32::new(6001).unwrap();
        let inner_tag = TagU32::new(6003).unwrap();

        let outer = message.get_field_map().get_group(outer_tag, 0).unwrap();
        let inner = outer.get_group(inner_tag, 0).unwrap();
        assert!(inner.get_field_map().get_raw(custom_tag).is_none());
        assert!(outer.get_field_map().get_raw(custom_tag).is_none());
        assert_eq!(message.get_field_map().get_raw(custom_tag).unwrap(), b"v");
    }

    #[test]
    fn test_required_field_missing_in_nested_group() {
        let dict = Dictionary::from_quickfix_spec(NESTED_REQUIRED_SPEC).unwrap();
        let raw = build_pipe_separated_message(
            "35=U1|49=S|56=T|34=1|52=20231103-12:00:00|6001=1|6002=od|6003=1|6004=id|",
        );
        let builder = MessageBuilder::new(dict, CONFIG).unwrap();
        let parsed = builder.build(&raw);

        assert!(matches!(
            parsed,
            ParsedMessage::Invalid {
                reason: InvalidReason::RequiredFieldMissing {
                    tag: 6005,
                    group_tag: Some(6003),
                },
                ..
            }
        ));
    }

    #[test]
    fn test_fields_declared_in_group_still_parsed_inside_when_validation_disabled() {
        // Same nested-group payload as `nested_repeating_group_entries`, but with
        // user-defined-field validation disabled. Tags that *are* declared inside
        // the group dictionary (including nested NoPartySubIDs) must still be
        // placed inside the group rather than leaking to the body.
        let raw = b"8=FIX.4.4|9=247|35=8|34=2|49=Broker|52=20231103-09:30:00|56=Client|11=Order12345|17=Exec12345|150=0|39=0|55=APPL|54=1|38=100|32=50|31=150.00|151=50|14=50|6=150.00|453=2|448=PARTYA|447=D|452=1|802=2|523=SUBPARTYA1|803=1|523=SUBPARTYA2|803=2|448=PARTYB|447=D|452=2|10=129|";
        let builder = MessageBuilder::new(Dictionary::fix44(), CONFIG_NO_USER_VALIDATION).unwrap();
        let message = builder.build(raw).into_message().unwrap();

        let party_a = message.get_group(fix44::NO_PARTY_I_DS, 0).unwrap();
        let sub = party_a
            .get_group(fix44::NO_PARTY_SUB_I_DS.tag(), 0)
            .unwrap();
        assert_eq!(sub.get::<&str>(fix44::PARTY_SUB_ID).unwrap(), "SUBPARTYA1");

        let party_b = message.get_group(fix44::NO_PARTY_I_DS, 1).unwrap();
        assert_eq!(party_b.get::<&str>(fix44::PARTY_ID).unwrap(), "PARTYB");
    }

    #[test]
    fn test_group_entry_missing_required_field_is_rejected() {
        let dict = Dictionary::fix44();
        let builder = MessageBuilder::new(dict, CONFIG).unwrap();

        // Find any (msg_type, group, required-non-delim-field) combination so
        // we can construct a minimal message that's structurally well-formed
        // except for the missing group field.
        let candidate = builder
            .message_specification
            .iter()
            .find_map(|(msg_type, message_def)| {
                message_def
                    .groups
                    .iter()
                    .find_map(|(group_tag, group_spec)| {
                        let required_non_delim = group_spec
                            .fields()
                            .iter()
                            .find(|f| f.is_required && f.tag != group_spec.delimiter_tag())?;
                        Some((
                            msg_type.clone(),
                            group_tag.get(),
                            group_spec.delimiter_tag().get(),
                            required_non_delim.tag.get(),
                        ))
                    })
            });

        let Some((msg_type, group_tag, delimiter_tag, missing_tag)) = candidate else {
            // The dictionary has no group with a required non-delimiter field.
            return;
        };

        let body = format!(
            "35={msg_type}|49=S|56=T|34=1|52=20231103-12:00:00|{group_tag}=1|{delimiter_tag}=X|"
        );
        let raw = build_pipe_separated_message(&body);
        let parsed = builder.build(&raw);

        match parsed {
            ParsedMessage::Invalid {
                reason:
                    InvalidReason::RequiredFieldMissing {
                        tag,
                        group_tag: Some(g),
                    },
                ..
            } => {
                assert_eq!(tag, missing_tag);
                assert_eq!(g, group_tag);
            }
            other => panic!(
                "expected RequiredFieldMissing(tag={missing_tag}, group_tag={group_tag}); \
                 msg_type={msg_type}, delimiter={delimiter_tag}, got: {}",
                match &other {
                    ParsedMessage::Valid(_) => "Valid".to_string(),
                    ParsedMessage::Invalid { reason, .. } => match reason {
                        InvalidReason::InvalidField(t) => format!("InvalidField({t})"),
                        InvalidReason::InvalidGroup(t) => format!("InvalidGroup({t})"),
                        InvalidReason::InvalidOrderInGroup { tag, group_tag } => {
                            format!("InvalidOrderInGroup(tag={tag}, group_tag={group_tag})")
                        }
                        InvalidReason::InvalidComponent(s) => format!("InvalidComponent({s})"),
                        InvalidReason::InvalidMsgType(s) => format!("InvalidMsgType({s})"),
                        InvalidReason::RequiredFieldMissing { tag, group_tag } => {
                            format!("RequiredFieldMissing(tag={tag}, group_tag={group_tag:?})")
                        }
                    },
                    ParsedMessage::Garbled(_) => "Garbled".to_string(),
                    ParsedMessage::UnexpectedError(_) => "UnexpectedError".to_string(),
                }
            ),
        }
    }
}
