use crate::error::{ParserError, ParserResult};

use anyhow::{Result, anyhow};
use hotfix_dictionary::{Dictionary, LayoutItem, LayoutItemKind, TagU32};
use std::collections::{HashMap, HashSet};

pub struct FieldDef {
    tag: TagU32,
    is_required: bool,
    is_group: bool,
}

pub struct GroupDef {
    starting_tag: TagU32,
    is_required: bool,
    fields: Vec<FieldDef>,
    nested_groups: HashMap<TagU32, GroupDef>,
}

impl GroupDef {
    pub fn contains_tag(&self, tag: TagU32) -> bool {
        self.fields.iter().any(|f| f.tag == tag)
    }

    pub fn get_nested_group(&self, tag: TagU32) -> Option<&GroupDef> {
        self.nested_groups.get(&tag)
    }
}

pub struct MessageDef {
    fields: Vec<FieldDef>,
    groups: HashMap<TagU32, GroupDef>,
}

impl MessageDef {
    pub fn contains_tag(&self, tag: TagU32) -> bool {
        self.fields.iter().any(|f| f.tag == tag)
    }

    pub fn get_group(&self, tag: TagU32) -> Option<&GroupDef> {
        self.groups.get(&tag)
    }
}

pub struct ParserDictionary<'a> {
    dict: &'a Dictionary,
    header_tags: HashSet<TagU32>,
    trailer_tags: HashSet<TagU32>,
    message_definitions: HashMap<String, MessageDef>,
}

impl<'a> ParserDictionary<'a> {
    pub fn new(dict: &'a Dictionary) -> Result<Self> {
        let parser = Self {
            dict,
            header_tags: Self::get_tags_for_component(dict, "StandardHeader")?,
            trailer_tags: Self::get_tags_for_component(dict, "StandardTrailer")?,
            message_definitions: Self::build_message_definitions(dict)?,
        };

        Ok(parser)
    }

    pub fn get_message_def(&self, msg_type: &str) -> ParserResult<&MessageDef> {
        match self.message_definitions.get(msg_type) {
            Some(message_def) => Ok(message_def),
            None => Err(ParserError::InvalidMsgType(msg_type.to_string())),
        }
    }

    fn build_message_definitions(dict: &Dictionary) -> Result<HashMap<String, MessageDef>> {
        let mut definitions = HashMap::new();

        for message in dict.messages() {
            let fields = message
                .layout()
                .flat_map(|item| extract_fields(dict, item))
                .flatten()
                .collect();

            let message_def = MessageDef {
                fields,
                groups: message.layout().fold(HashMap::new(), |mut acc, item| {
                    acc.extend(extract_groups(dict, item).unwrap());
                    acc
                }),
            };
            definitions.insert(message.msg_type().to_string(), message_def);
        }

        Ok(definitions)
    }

    fn get_tags_for_component(dict: &Dictionary, component_name: &str) -> Result<HashSet<TagU32>> {
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
}

fn extract_fields(dict: &Dictionary, item: LayoutItem) -> Result<Vec<FieldDef>> {
    let is_required = item.required();
    let fields = match item.kind() {
        LayoutItemKind::Component(c) => {
            let component = dict
                .component_by_name(c.name())
                .ok_or_else(|| anyhow!("missing component"))?;
            component
                .items()
                .flat_map(|i| extract_fields(dict, i))
                .flatten()
                .collect()
        }
        LayoutItemKind::Field(field) | LayoutItemKind::Group(field, _) => vec![FieldDef {
            tag: field.tag(),
            is_required,
            is_group: true,
        }],
    };

    Ok(fields)
}

fn extract_groups(dict: &Dictionary, item: LayoutItem) -> Result<HashMap<TagU32, GroupDef>> {
    let mut groups = HashMap::new();
    match item.kind() {
        LayoutItemKind::Component(c) => {
            let component = dict
                .component_by_name(c.name())
                .ok_or_else(|| anyhow!("missing component"))?;
            component.items().for_each(|i| {
                groups.extend(extract_groups(dict, i).unwrap());
            })
        }
        LayoutItemKind::Group(field, items) => {
            groups.insert(
                field.tag(),
                GroupDef {
                    starting_tag: field.tag(),
                    is_required: item.required(),
                    fields: extract_fields(dict, item.clone())?,
                    nested_groups: items.iter().fold(HashMap::new(), |mut acc, i| {
                        acc.extend(extract_groups(dict, i.clone()).unwrap());
                        acc
                    }),
                },
            );
        }
        _ => {}
    };

    Ok(groups)
}

#[cfg(test)]
mod tests {
    use crate::fix44;
    use crate::parser_dictionary::ParserDictionary;
    use hotfix_dictionary::{Dictionary, IsFieldDefinition, TagU32};

    #[test]
    fn test_top_level_fields() {
        let dict = Dictionary::fix44();
        let parser_dict = ParserDictionary::new(&dict).unwrap();

        // we take an `AllocationInstruction` message as an example
        let message_def = parser_dict.get_message_def("J").unwrap();

        // check that it contains `Symbol`, a tag from the nested `Instrument` component
        assert!(message_def.contains_tag(fix44::SYMBOL.tag()));

        // check that it contains `NoOrders`, the starting tag for `OrdAllocGrp`
        assert!(message_def.contains_tag(fix44::NO_ORDERS.tag()));

        // check that it doesn't contain other tags from the `OrdAllocGroup`
        assert!(!message_def.contains_tag(fix44::ORDER_QTY.tag()));
    }

    #[test]
    fn test_top_level_groups() {
        let dict = Dictionary::fix44();
        let parser_dict = ParserDictionary::new(&dict).unwrap();

        // we take an `AllocationInstruction` message as an example
        let message_def = parser_dict.get_message_def("J").unwrap();

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
}
