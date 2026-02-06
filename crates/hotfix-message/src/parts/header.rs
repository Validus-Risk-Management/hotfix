use hotfix_dictionary::IsFieldDefinition;

use crate::field_map::FieldMap;
use crate::parts::Part;
use crate::session_fields;

#[derive(Clone, Default)]
pub struct Header {
    pub fields: FieldMap,
}

impl Part for Header {
    fn get_field_map(&self) -> &FieldMap {
        &self.fields
    }

    fn get_field_map_mut(&mut self) -> &mut FieldMap {
        &mut self.fields
    }

    fn calculate_length(&self) -> usize {
        // when calculating the trailer's contribution to the message length,
        // the BeginString and BodyLength fields are not to be counted
        let skip = vec![
            session_fields::BEGIN_STRING.tag(),
            session_fields::BODY_LENGTH.tag(),
        ];
        self.fields.calculate_length(&skip)
    }
}
