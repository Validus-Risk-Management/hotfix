use hotfix_dictionary::TagU32;
use std::collections::BTreeMap;

struct Field {
    data: Vec<u8>,
}

#[derive(Default)]
pub struct FieldMap {
    fields: BTreeMap<TagU32, Field>,
}

impl FieldMap {
    fn insert(&mut self, tag: TagU32, data: Vec<u8>) {
        let field = Field { data };
        self.fields.insert(tag, field);
    }
}
