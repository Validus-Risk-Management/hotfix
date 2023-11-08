use crate::field_map::{Field, FieldMap};

mod body;
mod header;
mod repeating_group;
mod trailer;

use hotfix_dictionary::{IsFieldDefinition, TagU32};

pub(crate) use body::Body;
pub(crate) use header::Header;
use hotfix_encoding::field_access::{FieldType, FieldValueError};
use hotfix_encoding::HardCodedFixFieldDefinition;
pub(crate) use repeating_group::RepeatingGroup;
pub(crate) use trailer::Trailer;

// TODO: what a rubbish name.. but can't think of anything better that's not overloaded with fefix names
pub trait Part {
    fn get_field_map(&self) -> &FieldMap;
    fn get_field_map_mut(&mut self) -> &mut FieldMap;

    fn store_field(&mut self, field: Field) {
        self.get_field_map_mut().insert(field)
    }

    #[inline]
    fn get<'a, V>(
        &'a self,
        field: &HardCodedFixFieldDefinition,
    ) -> Result<V, FieldValueError<V::Error>>
    where
        V: FieldType<'a>,
    {
        self.get_raw(field)
            .map(V::deserialize)
            .transpose()
            .map_err(FieldValueError::Invalid)
            .and_then(|opt| opt.ok_or(FieldValueError::Missing))
    }

    #[inline]
    fn get_raw(&self, field: &HardCodedFixFieldDefinition) -> Option<&[u8]> {
        self.get_field_map().get_raw(field.tag())
    }

    fn pop(&mut self, tag: &TagU32) -> Option<Field> {
        self.get_field_map_mut().fields.remove(tag)
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
