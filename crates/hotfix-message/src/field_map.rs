use crate::Part;
use hotfix_dictionary::TagU32;
use hotfix_encoding::field_access::FieldType;
use std::collections::BTreeMap;

use crate::parts::RepeatingGroup;

pub struct Field {
    pub(crate) tag: TagU32,
    pub(crate) data: Vec<u8>,
}

impl Field {
    pub fn new(tag: TagU32, data: Vec<u8>) -> Self {
        Self { tag, data }
    }

    pub fn calculate_length(&self) -> usize {
        self.tag.to_bytes().len() + self.data.len() + 2
    }
}

#[derive(Default)]
pub struct FieldMap {
    pub fields: BTreeMap<TagU32, Field>,
    pub groups: BTreeMap<TagU32, Vec<RepeatingGroup>>,
}

impl FieldMap {
    pub fn insert(&mut self, field: Field) {
        self.fields.insert(field.tag, field);
    }

    pub fn set_groups(&mut self, start_tag: TagU32, groups: Vec<RepeatingGroup>) {
        self.groups.insert(start_tag, groups);
    }

    pub fn get(&self, tag: TagU32) -> Option<&Field> {
        self.fields.get(&tag)
    }

    pub fn get_group(&self, start_tag: TagU32, index: usize) -> Option<&RepeatingGroup> {
        self.groups
            .get(&start_tag)
            .and_then(|groups| groups.get(index))
    }

    pub fn calculate_length(&self, skip: &[TagU32]) -> usize {
        let fields_length: usize = self
            .fields
            .values()
            .filter(|f| !skip.contains(&f.tag))
            .map(|f| f.calculate_length())
            .sum();
        let groups_length: usize = self
            .groups
            .iter()
            .flat_map(|g| g.1)
            .map(|g| g.calculate_length())
            .sum();

        fields_length + groups_length
    }
}
