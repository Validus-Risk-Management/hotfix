use crate::field_map::{Field, FieldMap};

mod body;
mod header;
mod repeating_group;
mod trailer;

pub(crate) use body::Body;
pub(crate) use header::Header;
use hotfix_dictionary::TagU32;
pub(crate) use trailer::Trailer;

pub trait Part {
    fn get_field_map(&self) -> &FieldMap;
    fn get_field_map_mut(&mut self) -> &mut FieldMap;

    fn store_field(&mut self, field: Field) {
        self.get_field_map_mut().insert(field)
    }

    fn get(&self, tag: TagU32) -> Option<&Field> {
        self.get_field_map().get(tag)
    }
}
