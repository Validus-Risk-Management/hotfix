use crate::field_map::{Field, FieldMap};

mod body;
mod header;
mod repeating_group;
mod trailer;

use hotfix_dictionary::TagU32;

pub(crate) use body::Body;
pub(crate) use header::Header;
pub(crate) use repeating_group::RepeatingGroup;
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

    fn set_groups(&mut self, start_tag: TagU32, groups: Vec<RepeatingGroup>) {
        self.get_field_map_mut().set_groups(start_tag, groups);
    }

    fn get_group(&self, start_tag: TagU32, index: usize) -> Option<&RepeatingGroup> {
        self.get_field_map().get_group(start_tag, index)
    }

    fn calculate_length(&self) -> usize {
        self.get_field_map().calculate_length(&[])
    }
}
