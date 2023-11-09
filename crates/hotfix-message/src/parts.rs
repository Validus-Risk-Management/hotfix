use crate::field_map::{Field, FieldMap};
use std::collections::HashSet;

mod body;
mod header;
mod repeating_group;
mod trailer;

use hotfix_dictionary::{IsFieldDefinition, TagU32};

use crate::encoding::FieldValueError;
use crate::{FieldType, HardCodedFixFieldDefinition};
pub(crate) use body::Body;
pub(crate) use header::Header;
pub use repeating_group::RepeatingGroup;
pub(crate) use trailer::Trailer;

// TODO: what a rubbish name.. but can't think of anything better that's not overloaded with fefix names
pub trait Part {
    fn get_field_map(&self) -> &FieldMap;
    fn get_field_map_mut(&mut self) -> &mut FieldMap;

    fn set<'a, V>(&'a mut self, field_definition: &HardCodedFixFieldDefinition, value: V)
    where
        V: FieldType<'a>,
    {
        let tag = TagU32::new(field_definition.tag).unwrap();
        let field = Field::new(tag, value.to_bytes());

        self.store_field(field);
    }

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

    fn pop(&mut self, field: &HardCodedFixFieldDefinition) -> Option<Field> {
        self.get_field_map_mut().fields.remove(&field.tag())
    }

    fn set_groups(&mut self, groups: Vec<RepeatingGroup>) {
        let tags: HashSet<(TagU32, TagU32)> = groups
            .iter()
            .map(|g| (g.start_tag, g.delimiter_tag))
            .collect();
        assert_eq!(tags.len(), 1);
        let (start_tag, _) = tags.into_iter().next().unwrap();
        self.get_field_map_mut().set_groups(start_tag, groups);
    }

    fn get_group(&self, start_tag: TagU32, index: usize) -> Option<&RepeatingGroup> {
        self.get_field_map().get_group(start_tag, index)
    }

    fn calculate_length(&self) -> usize {
        self.get_field_map().calculate_length(&[])
    }
}
