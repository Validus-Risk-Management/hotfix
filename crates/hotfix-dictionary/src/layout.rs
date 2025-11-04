use std::fmt;

use crate::component::Component;
use crate::string::SmartString;
use crate::{Dictionary, Field};

pub fn display_layout_item(indent: u32, item: LayoutItem, f: &mut fmt::Formatter) -> fmt::Result {
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

#[derive(Clone, Debug)]
pub(crate) enum LayoutItemKindData {
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
pub(crate) struct LayoutItemData {
    pub(crate) required: bool,
    pub(crate) kind: LayoutItemKindData,
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
pub struct LayoutItem<'a>(pub(crate) &'a Dictionary, pub(crate) &'a LayoutItemData);

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

pub(crate) type LayoutItems = Vec<LayoutItemData>;
