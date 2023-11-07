use crate::field_map::FieldMap;
use crate::message::Config;
use std::io::Write;

pub trait Encode {
    fn write(&self, config: &Config, buffer: &mut Vec<u8>);
}

impl Encode for FieldMap {
    fn write(&self, config: &Config, buffer: &mut Vec<u8>) {
        for (tag, field) in &self.fields {
            let formatted_tag = format!("{}=", tag.get());
            buffer.write_all(formatted_tag.as_bytes()).unwrap();
            buffer.write_all(&field.data).unwrap();
            buffer.push(config.separator);

            if let Some(groups) = self.groups.get(tag) {
                for group in groups {
                    group.get_fields().write(config, buffer);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn encode_message_with_repeating_group() {
        todo!()
    }
}
