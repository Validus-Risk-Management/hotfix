use crate::field_map::FieldMap;
use crate::parts::Part;

#[derive(Default)]
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
}
