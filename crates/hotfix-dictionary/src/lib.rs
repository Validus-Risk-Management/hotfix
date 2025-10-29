//! Access to FIX Dictionary reference and message specifications.

mod builder;
mod component;
mod datatype;
mod dictionary;
mod field;
mod message_definition;
mod quickfix;

use crate::component::{Component, ComponentData};
pub use crate::dictionary::Dictionary;
pub use crate::field::{Field, IsFieldDefinition};
pub use datatype::{Datatype, FixDatatype};
use fnv::FnvHashMap;
use smartstring::alias::String as SmartString;
use std::fmt;
use std::sync::Arc;

/// A mapping from FIX version strings to [`Dictionary`] values.
pub type Dictionaries = FnvHashMap<String, Arc<Dictionary>>;

/// Type alias for FIX tags: 32-bit unsigned integers, strictly positive.
pub type TagU32 = std::num::NonZeroU32;

/// The expected location of a field within a FIX message (i.e. header, body, or trailer).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FieldLocation {
    /// The field is located inside the "Standard Header".
    Header,
    /// This field is located inside the body of the FIX message.
    Body,
    /// This field is located inside the "Standard Trailer".
    Trailer,
}

#[allow(dead_code)]
fn display_layout_item(indent: u32, item: LayoutItem, f: &mut fmt::Formatter) -> fmt::Result {
    for _ in 0..indent {
        write!(f, " ")?;
    }
    match item.kind() {
        LayoutItemKind::Field(_) => {
            writeln!(
                f,
                "<field name='{}' required='{}' />",
                item.tag_text(),
                item.required(),
            )?;
        }
        LayoutItemKind::Group(_, _fields) => {
            writeln!(
                f,
                "<group name='{}' required='{}' />",
                item.tag_text(),
                item.required(),
            )?;
            writeln!(f, "</group>")?;
        }
        LayoutItemKind::Component(_c) => {
            writeln!(
                f,
                "<component name='{}' required='{}' />",
                item.tag_text(),
                item.required(),
            )?;
            writeln!(f, "</component>")?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
struct DatatypeData {
    /// **Primary key.** Identifier of the datatype.
    datatype: FixDatatype,
    /// Human readable description of this Datatype.
    description: String,
    /// A string that contains examples values for a datatype
    examples: Vec<String>,
    // TODO: 'XML'.
}
/// A field is identified by a unique tag number and a name. Each field in a
/// message is associated with a value.
#[derive(Clone, Debug)]
pub struct FieldData {
    /// A human readable string representing the name of the field.
    name: SmartString,
    /// **Primary key.** A positive integer representing the unique
    /// identifier for this field type.
    tag: u32,
    /// The datatype of the field.
    data_type_name: SmartString,
    /// The associated data field. If given, this field represents the length of
    /// the referenced data field
    associated_data_tag: Option<usize>,
    value_restrictions: Option<Vec<FieldEnumData>>,
    /// Indicates whether the field is required in an XML message.
    required: bool,
    description: Option<String>,
}

#[derive(Clone, Debug)]
struct FieldEnumData {
    value: String,
    description: String,
}

/// A limitation imposed on the value of a specific FIX [`Field`].  Also known as
/// "code set".
#[derive(Debug)]
#[allow(dead_code)]
pub struct FieldEnum<'a>(&'a Dictionary, &'a FieldEnumData);

impl<'a> FieldEnum<'a> {
    /// Returns the string representation of this field variant.
    pub fn value(&self) -> &str {
        &self.1.value[..]
    }

    /// Returns the documentation description for `self`.
    pub fn description(&self) -> &str {
        &self.1.description[..]
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
enum LayoutItemKindData {
    Component {
        name: SmartString,
    },
    Group {
        len_field_tag: u32,
        items: Vec<LayoutItemData>,
    },
    Field {
        tag: u32,
    },
}

#[derive(Clone, Debug)]
struct LayoutItemData {
    required: bool,
    kind: LayoutItemKindData,
}

fn layout_item_kind<'a>(item: &'a LayoutItemKindData, dict: &'a Dictionary) -> LayoutItemKind<'a> {
    match item {
        LayoutItemKindData::Component { name } => {
            LayoutItemKind::Component(dict.component_by_name(name).unwrap())
        }
        LayoutItemKindData::Group {
            len_field_tag,
            items: items_data,
        } => {
            let items = items_data
                .iter()
                .map(|item_data| LayoutItem(dict, item_data))
                .collect::<Vec<_>>();
            let len_field = dict.field_by_tag(*len_field_tag).unwrap();
            LayoutItemKind::Group(len_field, items)
        }
        LayoutItemKindData::Field { tag } => {
            LayoutItemKind::Field(dict.field_by_tag(*tag).unwrap())
        }
    }
}

/// An entry in a sequence of FIX field definitions.
#[derive(Clone, Debug)]
pub struct LayoutItem<'a>(&'a Dictionary, &'a LayoutItemData);

/// The kind of element contained in a [`Message`].
#[derive(Debug)]
pub enum LayoutItemKind<'a> {
    /// This component item is another component.
    Component(Component<'a>),
    /// This component item is a FIX repeating group.
    Group(Field<'a>, Vec<LayoutItem<'a>>),
    /// This component item is a FIX field.
    Field(Field<'a>),
}

impl<'a> LayoutItem<'a> {
    /// Returns `true` if `self` is required in order to have a valid definition
    /// of its parent container, `false` otherwise.
    pub fn required(&self) -> bool {
        self.1.required
    }

    /// Returns the [`LayoutItemKind`] of `self`.
    pub fn kind(&self) -> LayoutItemKind<'_> {
        layout_item_kind(&self.1.kind, self.0)
    }

    /// Returns the human-readable name of `self`.
    pub fn tag_text(&self) -> String {
        match &self.1.kind {
            LayoutItemKindData::Component { name } => {
                self.0.component_by_name(name).unwrap().name().to_string()
            }
            LayoutItemKindData::Group {
                len_field_tag,
                items: _items,
            } => self
                .0
                .field_by_tag(*len_field_tag)
                .unwrap()
                .name()
                .to_string(),
            LayoutItemKindData::Field { tag } => {
                self.0.field_by_tag(*tag).unwrap().name().to_string()
            }
        }
    }
}

type LayoutItems = Vec<LayoutItemData>;
/// A [`Section`] is a collection of many [`Component`]-s. It has no practical
/// effect on encoding and decoding of FIX data and it's only used for
/// documentation and human readability.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {}

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn fix44_quickfix_is_ok() {
        let dict = Dictionary::fix44();
        let msg_heartbeat = dict.message_by_name("Heartbeat").unwrap();
        assert_eq!(msg_heartbeat.msg_type(), "0");
        assert_eq!(msg_heartbeat.name(), "Heartbeat".to_string());
        assert!(msg_heartbeat.layout().any(|c| {
            if let LayoutItemKind::Field(f) = c.kind() {
                f.name() == "TestReqID"
            } else {
                false
            }
        }));
    }

    #[test]
    fn all_datatypes_are_used_at_least_once() {
        for dict in Dictionary::common_dictionaries().iter() {
            let datatypes_count = dict.datatypes().len();
            let mut datatypes = HashSet::new();
            for field in dict.fields() {
                datatypes.insert(field.data_type().name().to_string());
            }
            assert_eq!(datatypes_count, datatypes.len());
        }
    }

    #[test]
    fn at_least_one_datatype() {
        for dict in Dictionary::common_dictionaries().iter() {
            assert!(!dict.datatypes().is_empty());
        }
    }

    #[test]
    fn std_header_and_trailer_always_present() {
        for dict in Dictionary::common_dictionaries().iter() {
            let std_header = dict.component_by_name("StandardHeader");
            let std_trailer = dict.component_by_name("StandardTrailer");
            assert!(std_header.is_some() && std_trailer.is_some());
        }
    }

    #[test]
    fn fix44_field_28_has_three_variants() {
        let dict = Dictionary::fix44();
        let field_28 = dict.field_by_tag(28).unwrap();
        assert_eq!(field_28.name(), "IOITransType");
        assert_eq!(field_28.enums().unwrap().count(), 3);
    }

    #[test]
    fn fix44_field_36_has_no_variants() {
        let dict = Dictionary::fix44();
        let field_36 = dict.field_by_tag(36).unwrap();
        assert_eq!(field_36.name(), "NewSeqNo");
        assert!(field_36.enums().is_none());
    }

    #[test]
    fn fix44_field_167_has_eucorp_variant() {
        let dict = Dictionary::fix44();
        let field_167 = dict.field_by_tag(167).unwrap();
        assert_eq!(field_167.name(), "SecurityType");
        assert!(field_167.enums().unwrap().any(|e| e.value() == "EUCORP"));
    }

    const INVALID_QUICKFIX_SPECS: &[&str] = &[
        include_str!("test_data/quickfix_specs/empty_file.xml"),
        include_str!("test_data/quickfix_specs/missing_components.xml"),
        include_str!("test_data/quickfix_specs/missing_fields.xml"),
        include_str!("test_data/quickfix_specs/missing_header.xml"),
        include_str!("test_data/quickfix_specs/missing_messages.xml"),
        include_str!("test_data/quickfix_specs/missing_trailer.xml"),
        include_str!("test_data/quickfix_specs/root_has_no_type_attr.xml"),
        include_str!("test_data/quickfix_specs/root_has_no_version_attrs.xml"),
        include_str!("test_data/quickfix_specs/root_is_not_fix.xml"),
    ];

    #[test]
    fn invalid_quickfix_specs() {
        for spec in INVALID_QUICKFIX_SPECS.iter() {
            let dict = Dictionary::from_quickfix_spec(spec);
            assert!(dict.is_err(), "{}", spec);
        }
    }
}
