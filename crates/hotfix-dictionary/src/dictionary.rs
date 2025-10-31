use crate::{Component, ComponentData, Datatype, DatatypeData, Field, FieldData};

use crate::message_definition::{MessageData, MessageDefinition};
use crate::quickfix::{ParseDictionaryError, QuickFixReader};
use fnv::FnvHashMap;
use smartstring::alias::String as SmartString;

/// Specifies business semantics for application-level entities within the FIX
/// Protocol.
///
/// You can rely on [`Dictionary`] for accessing details about
/// fields, messages, and other abstract entities as defined in the FIX
/// specifications. Examples of such information include:
///
/// - The mapping of FIX field names to numeric tags (e.g. `BeginString` is 8).
/// - Which FIX fields are mandatory and which are optional.
/// - The data type of each and every FIX field.
/// - What fields to expect in FIX headers.
///
/// N.B. The FIX Protocol mandates separation of concerns between session and
/// application protocol only for FIX 5.0 and subsequent versions. All FIX
/// Dictionaries for older versions will also contain information about the
/// session layer.
#[derive(Debug, Clone)]
pub struct Dictionary {
    pub(crate) version: String,

    pub(crate) data_types_by_name: FnvHashMap<SmartString, DatatypeData>,

    pub(crate) fields_by_tags: FnvHashMap<u32, FieldData>,
    pub(crate) field_tags_by_name: FnvHashMap<SmartString, u32>,

    pub(crate) components_by_name: FnvHashMap<SmartString, ComponentData>,

    pub(crate) messages_by_msgtype: FnvHashMap<SmartString, MessageData>,
    pub(crate) message_msgtypes_by_name: FnvHashMap<SmartString, SmartString>,
}

impl Dictionary {
    /// Creates a new empty FIX Dictionary named `version`.
    pub fn new<S: ToString>(version: S) -> Self {
        Dictionary {
            version: version.to_string(),
            data_types_by_name: FnvHashMap::default(),
            fields_by_tags: FnvHashMap::default(),
            field_tags_by_name: FnvHashMap::default(),
            components_by_name: FnvHashMap::default(),
            messages_by_msgtype: FnvHashMap::default(),
            message_msgtypes_by_name: FnvHashMap::default(),
        }
    }

    /// Attempts to read a QuickFIX-style specification file and convert it into
    /// a [`Dictionary`].
    pub fn from_quickfix_spec(input: &str) -> Result<Self, ParseDictionaryError> {
        let xml_document =
            roxmltree::Document::parse(input).map_err(|_| ParseDictionaryError::InvalidFormat)?;
        QuickFixReader::new(&xml_document)
    }

    /// Returns the version string associated with this [`Dictionary`] (e.g.
    /// `FIXT.1.1`, `FIX.4.2`).
    ///
    /// ```
    /// use hotfix_dictionary::Dictionary;
    ///
    /// let dict = Dictionary::fix44();
    /// assert_eq!(dict.version(), "FIX.4.4");
    /// ```
    pub fn version(&self) -> &str {
        self.version.as_str()
    }

    pub fn load_from_file(path: &str) -> Result<Self, ParseDictionaryError> {
        let spec = std::fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("unable to read FIX dictionary file at {path}"));
        Dictionary::from_quickfix_spec(&spec)
    }

    /// Creates a new [`Dictionary`] for FIX 4.0.
    #[cfg(feature = "fix40")]
    #[cfg_attr(doc_cfg, doc(cfg(feature = "fix40")))]
    pub fn fix40() -> Self {
        let spec = include_str!("resources/quickfix/FIX-4.0.xml");
        Dictionary::from_quickfix_spec(spec).unwrap()
    }

    /// Creates a new [`Dictionary`] for FIX 4.1.
    #[cfg(feature = "fix41")]
    #[cfg_attr(doc_cfg, doc(cfg(feature = "fix41")))]
    pub fn fix41() -> Self {
        let spec = include_str!("resources/quickfix/FIX-4.1.xml");
        Dictionary::from_quickfix_spec(spec).unwrap()
    }

    /// Creates a new [`Dictionary`] for FIX 4.2.
    #[cfg(feature = "fix42")]
    #[cfg_attr(doc_cfg, doc(cfg(feature = "fix42")))]
    pub fn fix42() -> Self {
        let spec = include_str!("resources/quickfix/FIX-4.2.xml");
        Dictionary::from_quickfix_spec(spec).unwrap()
    }

    /// Creates a new [`Dictionary`] for FIX 4.3.
    #[cfg(feature = "fix43")]
    #[cfg_attr(doc_cfg, doc(cfg(feature = "fix43")))]
    pub fn fix43() -> Self {
        let spec = include_str!("resources/quickfix/FIX-4.3.xml");
        Dictionary::from_quickfix_spec(spec).unwrap()
    }

    /// Creates a new [`Dictionary`] for FIX 4.4.
    pub fn fix44() -> Self {
        let spec = include_str!("resources/quickfix/FIX-4.4.xml");
        Dictionary::from_quickfix_spec(spec).unwrap()
    }

    /// Creates a new [`Dictionary`] for FIX 5.0.
    #[cfg(feature = "fix50")]
    #[cfg_attr(doc_cfg, doc(cfg(feature = "fix50")))]
    pub fn fix50() -> Self {
        let spec = include_str!("resources/quickfix/FIX-5.0.xml");
        Dictionary::from_quickfix_spec(spec).unwrap()
    }

    /// Creates a new [`Dictionary`] for FIX 5.0 SP1.
    #[cfg(feature = "fix50sp1")]
    #[cfg_attr(doc_cfg, doc(cfg(feature = "fix50sp1")))]
    pub fn fix50sp1() -> Self {
        let spec = include_str!("resources/quickfix/FIX-5.0-SP1.xml");
        Dictionary::from_quickfix_spec(spec).unwrap()
    }

    /// Creates a new [`Dictionary`] for FIX 5.0 SP2.
    #[cfg(feature = "fix50sp2")]
    #[cfg_attr(doc_cfg, doc(cfg(feature = "fix50sp1")))]
    pub fn fix50sp2() -> Self {
        let spec = include_str!("resources/quickfix/FIX-5.0-SP2.xml");
        Dictionary::from_quickfix_spec(spec).unwrap()
    }

    /// Creates a new [`Dictionary`] for FIXT 1.1.
    #[cfg(feature = "fixt11")]
    #[cfg_attr(doc_cfg, doc(cfg(feature = "fixt11")))]
    pub fn fixt11() -> Self {
        let spec = include_str!("resources/quickfix/FIXT-1.1.xml");
        Dictionary::from_quickfix_spec(spec).unwrap()
    }

    /// Returns a [`Vec`] of FIX [`Dictionary`]'s for the most common FIX
    /// versions (that have been enabled via feature flags). This is only
    /// intended for testing purposes.
    pub fn common_dictionaries() -> Vec<Dictionary> {
        vec![
            #[cfg(feature = "fix40")]
            Self::fix40(),
            #[cfg(feature = "fix41")]
            Self::fix41(),
            #[cfg(feature = "fix42")]
            Self::fix42(),
            #[cfg(feature = "fix43")]
            Self::fix43(),
            Self::fix44(),
            #[cfg(feature = "fix50")]
            Self::fix50(),
            #[cfg(feature = "fix50sp1")]
            Self::fix50sp1(),
            #[cfg(feature = "fix50sp2")]
            Self::fix50sp2(),
            #[cfg(feature = "fixt11")]
            Self::fixt11(),
        ]
    }

    /// Returns the [`Message`] associated with `name`, if any.
    ///
    /// ```
    /// use hotfix_dictionary::Dictionary;
    ///
    /// let dict = Dictionary::fix44();
    ///
    /// let msg1 = dict.message_by_name("Heartbeat").unwrap();
    /// let msg2 = dict.message_by_msgtype("0").unwrap();
    /// assert_eq!(msg1.name(), msg2.name());
    /// ```
    pub fn message_by_name(&self, name: &str) -> Option<MessageDefinition<'_>> {
        let msg_type = self.message_msgtypes_by_name.get(name)?;
        self.message_by_msgtype(msg_type)
    }

    /// Returns the [`Message`] that has the given `msgtype`, if any.
    ///
    /// ```
    /// use hotfix_dictionary::Dictionary;
    ///
    /// let dict = Dictionary::fix44();
    ///
    /// let msg1 = dict.message_by_msgtype("0").unwrap();
    /// let msg2 = dict.message_by_name("Heartbeat").unwrap();
    /// assert_eq!(msg1.name(), msg2.name());
    /// ```
    pub fn message_by_msgtype(&self, msgtype: &str) -> Option<MessageDefinition<'_>> {
        self.messages_by_msgtype
            .get(msgtype)
            .map(|data| MessageDefinition(self, data))
    }

    /// Returns the [`Component`] named `name`, if any.
    pub fn component_by_name(&self, name: &str) -> Option<Component<'_>> {
        self.components_by_name
            .get(name)
            .map(|data| Component(self, data))
    }

    /// Returns the [`Datatype`] named `name`, if any.
    ///
    /// ```
    /// use hotfix_dictionary::Dictionary;
    ///
    /// let dict = Dictionary::fix44();
    /// let dt = dict.datatype_by_name("String").unwrap();
    /// assert_eq!(dt.name(), "String");
    /// ```
    pub fn datatype_by_name(&self, name: &str) -> Option<Datatype<'_>> {
        self.data_types_by_name
            .get(name)
            .map(|data| Datatype(self, data))
    }

    /// Returns the [`Field`] associated with `tag`, if any.
    ///
    /// ```
    /// use hotfix_dictionary::Dictionary;
    ///
    /// let dict = Dictionary::fix44();
    /// let field1 = dict.field_by_tag(112).unwrap();
    /// let field2 = dict.field_by_name("TestReqID").unwrap();
    /// assert_eq!(field1.name(), field2.name());
    /// ```
    pub fn field_by_tag(&self, tag: u32) -> Option<Field<'_>> {
        self.fields_by_tags
            .get(&tag)
            .map(|data| Field::new(self, data))
    }

    /// Returns the [`Field`] named `name`, if any.
    pub fn field_by_name(&self, name: &str) -> Option<Field<'_>> {
        let tag = self.field_tags_by_name.get(name)?;
        self.field_by_tag(*tag)
    }

    /// Returns a [`Vec`] of all [`Datatype`]'s in this [`Dictionary`]. The ordering
    /// of items is not specified.
    ///
    /// ```
    /// use hotfix_dictionary::Dictionary;
    ///
    /// let dict = Dictionary::fix44();
    /// // FIX 4.4 defines 23 (FIXME) datatypes.
    /// assert_eq!(dict.datatypes().len(), 23);
    /// ```
    pub fn datatypes(&self) -> Vec<Datatype<'_>> {
        self.data_types_by_name
            .values()
            .map(|data| Datatype(self, data))
            .collect()
    }

    /// Returns a [`Vec`] of all [`Message`]'s in this [`Dictionary`]. The ordering
    /// of items is not specified.
    ///
    /// ```
    /// use hotfix_dictionary::Dictionary;
    ///
    /// let dict = Dictionary::fix44();
    /// let msgs = dict.messages();
    /// let msg = msgs.iter().find(|m| m.name() == "MarketDataRequest");
    /// assert_eq!(msg.unwrap().msg_type(), "V");
    /// ```
    pub fn messages(&self) -> Vec<MessageDefinition<'_>> {
        self.messages_by_msgtype
            .values()
            .map(|data| MessageDefinition(self, data))
            .collect()
    }

    /// Returns a [`Vec`] of all [`Field`]'s in this [`Dictionary`]. The ordering
    /// of items is not specified.
    pub fn fields(&self) -> Vec<Field<'_>> {
        self.fields_by_tags
            .values()
            .map(|data| Field::new(self, data))
            .collect()
    }

    /// Returns a [`Vec`] of all [`Component`]'s in this [`Dictionary`]. The ordering
    /// of items is not specified.
    pub fn components(&self) -> Vec<Component<'_>> {
        self.components_by_name
            .values()
            .map(|data| Component(self, data))
            .collect()
    }
}
