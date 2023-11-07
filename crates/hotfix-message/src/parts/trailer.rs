use crate::field_map::FieldMap;
use crate::parts::Part;
use hotfix_dictionary::TagU32;

#[derive(Default)]
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
        let skip = vec![TagU32::new(10).unwrap()];
        self.fields.calculate_length(&skip)
    }
}
