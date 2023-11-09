use crate::field_map::FieldMap;
use crate::parts::Part;
use crate::HardCodedFixFieldDefinition;
use hotfix_dictionary::{IsFieldDefinition, TagU32};

#[allow(dead_code)]
pub struct RepeatingGroup {
    pub(crate) start_tag: TagU32,
    pub(crate) delimiter_tag: TagU32,
    fields: FieldMap,
}

impl RepeatingGroup {
    pub fn new(
        start_tag: &HardCodedFixFieldDefinition,
        delimiter_tag: &HardCodedFixFieldDefinition,
    ) -> Self {
        Self::new_with_tags(start_tag.tag(), delimiter_tag.tag())
    }

    pub(crate) fn new_with_tags(start_tag: TagU32, delimiter_tag: TagU32) -> Self {
        Self {
            start_tag,
            delimiter_tag,
            fields: FieldMap::default(),
        }
    }

    pub fn get_fields(&self) -> &FieldMap {
        &self.fields
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
