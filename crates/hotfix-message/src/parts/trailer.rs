use crate::field_map::FieldMap;
use crate::parts::Part;
use crate::session_fields;
use hotfix_dictionary::IsFieldDefinition;

#[derive(Clone, Default)]
pub struct Trailer {
    pub(crate) fields: FieldMap,
}

impl Part for Trailer {
    fn get_field_map(&self) -> &FieldMap {
        &self.fields
    }

    fn get_field_map_mut(&mut self) -> &mut FieldMap {
        &mut self.fields
    }

    fn calculate_length(&self) -> usize {
        // when calculating the trailer's contribution to the message length,
        // the checksum itself must be skipped to avoid a circular dependency
        let skip = vec![session_fields::CHECK_SUM.tag()];
        self.fields.calculate_length(&skip)
    }
}
