use crate::message_definition::MessageData;
use crate::{ComponentData, DatatypeData, Dictionary, FieldData};

pub struct DictionaryBuilder {
    dict: Dictionary,
}

impl DictionaryBuilder {
    pub fn new<S: ToString>(version: S) -> Self {
        Self {
            dict: Dictionary::new(version),
        }
    }

    pub fn dict(&self) -> &Dictionary {
        &self.dict
    }

    pub(crate) fn add_field(&mut self, field: FieldData) {
        self.dict
            .field_tags_by_name
            .insert(field.name.clone(), field.tag);
        self.dict.fields_by_tags.insert(field.tag, field);
    }

    pub(crate) fn add_message(&mut self, message: MessageData) {
        self.dict
            .message_msgtypes_by_name
            .insert(message.name.clone(), message.msg_type.clone());
        self.dict
            .messages_by_msgtype
            .insert(message.msg_type.clone(), message);
    }

    pub(crate) fn add_component(&mut self, component: ComponentData) {
        self.dict
            .components_by_name
            .insert(component.name.clone(), component);
    }

    pub(crate) fn add_datatype(&mut self, datatype: DatatypeData) {
        self.dict
            .data_types_by_name
            .insert(datatype.datatype.name().into(), datatype);
    }
}

impl From<DictionaryBuilder> for Dictionary {
    fn from(builder: DictionaryBuilder) -> Self {
        builder.dict
    }
}
