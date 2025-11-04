use crate::string::SmartString;
use crate::{Dictionary, LayoutItem, LayoutItems};

#[derive(Clone, Debug)]
pub struct MessageData {
    /// The unique integer identifier of this message type.
    pub(crate) component_id: u32,
    /// **Primary key**. The unique character identifier of this message
    /// type; used literally in FIX messages.
    pub(crate) msg_type: SmartString,
    /// The name of this message type.
    pub(crate) name: SmartString,
    pub(crate) layout_items: LayoutItems,
    /// A boolean used to indicate if the message is to be generated as part
    /// of FIXML.
    pub(crate) required: bool,
    pub(crate) description: String,
}

/// A [`MessageDefinition`] is a unit of information sent on the wire between
/// counterparties. Every [`MessageDefinition`] is composed of fields and/or components.
#[derive(Debug)]
pub struct MessageDefinition<'a>(pub(crate) &'a Dictionary, pub(crate) &'a MessageData);

impl<'a> MessageDefinition<'a> {
    /// Returns the human-readable name of `self`.
    pub fn name(&self) -> &str {
        self.1.name.as_str()
    }

    /// Returns the message type of `self`.
    pub fn msg_type(&self) -> &str {
        self.1.msg_type.as_str()
    }

    /// Returns the description associated with `self`.
    pub fn description(&self) -> &str {
        &self.1.description
    }

    /// Returns the component ID of `self`.
    pub fn component_id(&self) -> u32 {
        self.1.component_id
    }

    pub fn layout(&self) -> impl Iterator<Item = LayoutItem<'_>> {
        self.1
            .layout_items
            .iter()
            .map(move |data| LayoutItem(self.0, data))
    }

    pub fn fixml_required(&self) -> bool {
        self.1.required
    }
}
