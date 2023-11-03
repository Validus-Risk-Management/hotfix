use crate::field_map::FieldMap;
use hotfix_dictionary::TagU32;

#[allow(dead_code)]
struct RepeatingGroup {
    tag: TagU32,
    delimiter_tag: TagU32,
    fields: FieldMap,
}
