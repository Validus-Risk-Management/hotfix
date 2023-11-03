use crate::field_map::FieldMap;
use crate::parts::Part;
use hotfix_dictionary::TagU32;

#[allow(dead_code)]
struct RepeatingGroup {
    start_tag: TagU32,
    delimiter_tag: TagU32,
    fields: FieldMap,
}

impl RepeatingGroup {
    pub fn new(start_tag: TagU32, delimiter_tag: TagU32) -> Self {
        Self {
            start_tag,
            delimiter_tag,
            fields: FieldMap::default(),
        }
    }
}

impl Part for RepeatingGroup {
    fn get_field_map(&self) -> &FieldMap {
        &self.fields
    }

    fn get_field_map_mut(&mut self) -> &mut FieldMap {
        &mut self.fields
    }
}
