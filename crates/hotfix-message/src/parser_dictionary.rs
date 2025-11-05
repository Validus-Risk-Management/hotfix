use crate::error::{ParserError, ParserResult};
use crate::message::Config;

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
                groups: Default::default(),
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

#[cfg(test)]
mod tests {
    use crate::fix44;
    use crate::parser_dictionary::ParserDictionary;
    use hotfix_dictionary::{Dictionary, IsFieldDefinition, TagU32};

    #[test]
    fn test_nested_groups_are_represented_correctly() {
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
}
