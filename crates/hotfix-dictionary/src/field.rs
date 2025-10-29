use crate::{Datatype, Dictionary, FixDatatype, TagU32};
use smartstring::alias::String as SmartString;

pub trait IsFieldDefinition {
    /// Returns the FIX tag associated with `self`.
    fn tag(&self) -> TagU32;

    /// Returns the official, ASCII, human-readable name associated with `self`.
    fn name(&self) -> &str;

    /// Returns the field location of `self`.
    fn location(&self) -> FieldLocation;
}

/// A field is the most granular message structure abstraction. It carries a
/// specific business meaning as described by the FIX specifications. The data
/// domain of a [`Field`] is either a [`Datatype`] or a "code set", i.e.
/// enumeration.
#[derive(Debug, Copy, Clone)]
pub struct Field<'a>(&'a Dictionary, &'a FieldData);

/// A field is identified by a unique tag number and a name. Each field in a
/// message is associated with a value.
#[derive(Clone, Debug)]
pub struct FieldData {
    /// A human readable string representing the name of the field.
    pub(crate) name: SmartString,
    /// **Primary key.** A positive integer representing the unique
    /// identifier for this field type.
    pub(crate) tag: u32,
    /// The datatype of the field.
    pub(crate) data_type_name: SmartString,
    /// The associated data field. If given, this field represents the length of
    /// the referenced data field
    pub(crate) associated_data_tag: Option<usize>,
    pub(crate) value_restrictions: Option<Vec<FieldEnumData>>,
    /// Indicates whether the field is required in an XML message.
    pub(crate) required: bool,
    pub(crate) description: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct FieldEnumData {
    pub(crate) value: String,
    pub(crate) description: String,
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

impl<'a> Field<'a> {
    pub fn new(dictionary: &'a Dictionary, field_data: &'a FieldData) -> Self {
        Self(dictionary, field_data)
    }

    pub fn doc_url_onixs(&self, version: &str) -> String {
        let v = match version {
            "FIX.4.0" => "4.0",
            "FIX.4.1" => "4.1",
            "FIX.4.2" => "4.2",
            "FIX.4.3" => "4.3",
            "FIX.4.4" => "4.4",
            "FIX.5.0" => "5.0",
            "FIX.5.0SP1" => "5.0.SP1",
            "FIX.5.0SP2" => "5.0.SP2",
            "FIXT.1.1" => "FIXT.1.1",
            s => s,
        };
        format!(
            "https://www.onixs.biz/fix-dictionary/{}/tagNum_{}.html",
            v,
            self.1.tag.to_string().as_str()
        )
    }

    pub fn is_num_in_group(&self) -> bool {
        fn nth_char_is_uppercase(s: &str, i: usize) -> bool {
            s.chars().nth(i).map(|c| c.is_ascii_uppercase()) == Some(true)
        }

        self.fix_datatype().base_type() == FixDatatype::NumInGroup
            || self.name().ends_with("Len")
            || (self.name().starts_with("No") && nth_char_is_uppercase(self.name(), 2))
    }

    /// Returns the [`FixDatatype`] of `self`.
    pub fn fix_datatype(&self) -> FixDatatype {
        self.data_type().basetype()
    }

    /// Returns the name of `self`. Field names are unique across each FIX
    /// [`Dictionary`].
    pub fn name(&self) -> &str {
        self.1.name.as_str()
    }

    /// Returns the numeric tag of `self`. Field tags are unique across each FIX
    /// [`Dictionary`].
    pub fn tag(&self) -> TagU32 {
        TagU32::new(self.1.tag).unwrap()
    }

    /// In case this field allows any value, it returns `None`; otherwise; it
    /// returns an [`Iterator`] of all allowed values.
    pub fn enums(&self) -> Option<impl Iterator<Item = FieldEnum<'_>>> {
        self.1
            .value_restrictions
            .as_ref()
            .map(move |v| v.iter().map(move |f| FieldEnum(self.0, f)))
    }

    /// Returns the [`Datatype`] of `self`.
    pub fn data_type(&self) -> Datatype<'_> {
        self.0
            .datatype_by_name(self.1.data_type_name.as_str())
            .unwrap()
    }

    pub fn data_tag(&self) -> Option<TagU32> {
        self.1
            .associated_data_tag
            .map(|tag| TagU32::new(tag as u32).unwrap())
    }

    pub fn required_in_xml_messages(&self) -> bool {
        self.1.required
    }

    pub fn description(&self) -> Option<&str> {
        self.1.description.as_deref()
    }
}

impl<'a> IsFieldDefinition for Field<'a> {
    fn tag(&self) -> TagU32 {
        TagU32::new(self.1.tag).expect("Invalid FIX tag (0)")
    }

    fn name(&self) -> &str {
        self.1.name.as_str()
    }

    fn location(&self) -> FieldLocation {
        FieldLocation::Body // FIXME
    }
}

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
