use hotfix_dictionary::TagU32;
use std::collections::BTreeMap;

pub struct Field {
    pub(crate) tag: TagU32,
    pub(crate) data: Vec<u8>,
}

impl Field {
    pub fn new(tag: TagU32, data: Vec<u8>) -> Self {
        Self { tag, data }
    }
}

#[derive(Default)]
pub struct FieldMap {
    fields: BTreeMap<TagU32, Field>,
}

impl FieldMap {
    pub fn insert(&mut self, field: Field) {
        self.fields.insert(field.tag, field);
    }

    pub fn get(&self, tag: TagU32) -> Option<&Field> {
        self.fields.get(&tag)
    }
}
