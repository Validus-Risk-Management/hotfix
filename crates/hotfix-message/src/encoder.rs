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
    use crate::message::{Config, Message};
    use hotfix_dictionary::Dictionary;
    use hotfix_encoding::field_types::Timestamp;
    use hotfix_encoding::fix44;

    #[test]
    fn encode_simple_message() {
        let mut msg = Message::new("FIX.4.4", "D");
        msg.set(fix44::MSG_SEQ_NUM, 1);
        msg.set(fix44::SENDER_COMP_ID, "CLIENT_A");
        msg.set(fix44::TARGET_COMP_ID, "BROKER_B");
        msg.set(fix44::SENDING_TIME, Timestamp::utc_now());
        msg.set(fix44::CL_ORD_ID, "0001");
        msg.set(fix44::SYMBOL, "AAPL");
        msg.set(fix44::SIDE, fix44::Side::Buy);
        msg.set(fix44::TRANSACT_TIME, Timestamp::utc_now());
        msg.set(fix44::ORD_TYPE, fix44::OrdType::Limit);
        msg.set(fix44::PRICE, 150);
        msg.set(fix44::ORDER_QTY, 60);

        let config = Config::default();
        let raw_message = msg.encode(&config);

        let dict = Dictionary::fix44();
        let parsed_message = Message::from_bytes(config, &dict, &raw_message);

        let symbol = parsed_message.get(fix44::SYMBOL).unwrap();
        assert_eq!(symbol, b"AAPL");

        let qty = parsed_message.get(fix44::ORDER_QTY).unwrap();
        assert_eq!(qty, b"60");
    }

    #[test]
    fn encode_message_with_repeating_group() {
        // TODO
    }
}
