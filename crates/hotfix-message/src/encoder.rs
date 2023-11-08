use crate::field_map::{Field, FieldMap};
use crate::message::Config;
use hotfix_dictionary::TagU32;
use std::io::Write;

pub trait Encode {
    fn write(&self, config: &Config, buffer: &mut Vec<u8>, pre_fields: &[TagU32]);
}

impl Encode for FieldMap {
    fn write(&self, config: &Config, buffer: &mut Vec<u8>, pre_fields: &[TagU32]) {
        let mut write_field = |field: &Field| {
            let formatted_tag = format!("{}=", field.tag.get());
            buffer.write_all(formatted_tag.as_bytes()).unwrap();
            buffer.write_all(&field.data).unwrap();
            buffer.push(config.separator);

            if let Some(groups) = self.groups.get(&field.tag) {
                for group in groups {
                    group.get_fields().write(config, buffer, &[]);
                }
            }
        };

        for pre_field_tag in pre_fields {
            if let Some(field) = self.fields.get(pre_field_tag) {
                write_field(field);
            }
        }

        for (tag, field) in &self.fields {
            if !pre_fields.contains(tag) {
                write_field(field);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::field_map::Field;
    use crate::message::{Config, Message};
    use crate::parts::RepeatingGroup;
    use crate::Part;
    use hotfix_dictionary::{Dictionary, IsFieldDefinition};
    use hotfix_encoding::field_types::{Date, Time, Timestamp};
    use hotfix_encoding::fix44;

    #[test]
    fn encode_simple_message() {
        let mut msg = Message::new("FIX.4.4", "D");
        msg.set(fix44::MSG_SEQ_NUM, 1);
        msg.set(fix44::SENDER_COMP_ID, "CLIENT_A");
        msg.set(fix44::TARGET_COMP_ID, "BROKER_B");
        msg.set(fix44::SENDING_TIME, Timestamp::utc_now());
        msg.set(fix44::CL_ORD_ID, "ORDER_0001");
        msg.set(fix44::SYMBOL, "AAPL");
        msg.set(fix44::SIDE, fix44::Side::Buy);
        msg.set(fix44::TRANSACT_TIME, Timestamp::utc_now());
        msg.set(fix44::ORD_TYPE, fix44::OrdType::Limit);
        msg.set(fix44::PRICE, 150);
        msg.set(fix44::ORDER_QTY, 60);

        let config = Config { separator: b'|' };
        let raw_message = msg.encode(&config);

        let dict = Dictionary::fix44();
        let parsed_message = Message::from_bytes(config, &dict, &raw_message);

        let symbol = parsed_message.get_raw(fix44::SYMBOL).unwrap();
        assert_eq!(symbol, b"AAPL");

        let qty = parsed_message.get_raw(fix44::ORDER_QTY).unwrap();
        assert_eq!(qty, b"60");

        let qty = parsed_message.get_raw(fix44::BODY_LENGTH).unwrap();
        assert_eq!(qty, b"129");
    }

    #[test]
    fn encode_message_with_repeating_group() {
        let sending_time = Timestamp::new(
            Date::new(2023, 11, 7).unwrap(),
            Time::from_hmsm(11, 0, 0, 0).unwrap(),
        );
        let mut msg = Message::new("FIX.4.4", "8");
        msg.set(fix44::MSG_SEQ_NUM, 1);
        msg.set(fix44::SENDER_COMP_ID, "BROKER_B");
        msg.set(fix44::TARGET_COMP_ID, "CLIENT_A");
        msg.set(fix44::SENDING_TIME, sending_time);
        msg.set(fix44::CL_ORD_ID, "ORDER_0001");
        msg.set(fix44::EXEC_ID, "Exec12345");
        msg.set(fix44::ORD_STATUS, "0");
        msg.set(fix44::SYMBOL, "AAPL");
        msg.set(fix44::SIDE, fix44::Side::Buy);
        msg.set(fix44::ORDER_QTY, 1000);
        msg.set(fix44::LAST_QTY, 200);
        msg.set(fix44::LAST_PX, 150.0);
        msg.set(fix44::LEAVES_QTY, 800);
        msg.set(fix44::CUM_QTY, 200);
        msg.set(fix44::AVG_PX, 150.0);

        msg.set(fix44::NO_PARTY_I_DS, 2);

        let mut party_1 = RepeatingGroup::new(fix44::NO_PARTY_I_DS.tag(), fix44::PARTY_ID.tag());
        party_1.store_field(Field::new(fix44::PARTY_ID.tag(), b"PARTY_A".to_vec()));
        party_1.store_field(Field::new(fix44::PARTY_ID_SOURCE.tag(), b"D".to_vec()));
        party_1.store_field(Field::new(fix44::PARTY_ROLE.tag(), b"1".to_vec()));
        party_1.store_field(Field::new(fix44::NO_PARTY_SUB_I_DS.tag(), b"2".to_vec()));

        let mut subparty_1 =
            RepeatingGroup::new(fix44::NO_PARTY_SUB_I_DS.tag(), fix44::PARTY_SUB_ID.tag());
        subparty_1.store_field(Field::new(
            fix44::PARTY_SUB_ID.tag(),
            b"SUBPARTY_A_1".to_vec(),
        ));
        subparty_1.store_field(Field::new(fix44::PARTY_SUB_ID_TYPE.tag(), b"1".to_vec()));

        let mut subparty_2 =
            RepeatingGroup::new(fix44::NO_PARTY_SUB_I_DS.tag(), fix44::PARTY_SUB_ID.tag());
        subparty_2.store_field(Field::new(
            fix44::PARTY_SUB_ID.tag(),
            b"SUBPARTY_A_2".to_vec(),
        ));
        subparty_2.store_field(Field::new(fix44::PARTY_SUB_ID_TYPE.tag(), b"2".to_vec()));

        party_1.set_groups(fix44::NO_PARTY_SUB_I_DS.tag(), vec![subparty_1, subparty_2]);

        let mut party_2 = RepeatingGroup::new(fix44::NO_PARTY_I_DS.tag(), fix44::PARTY_ID.tag());
        party_2.store_field(Field::new(fix44::PARTY_ID.tag(), b"PARTY_B".to_vec()));
        party_2.store_field(Field::new(fix44::PARTY_ID_SOURCE.tag(), b"D".to_vec()));
        party_2.store_field(Field::new(fix44::PARTY_ROLE.tag(), b"2".to_vec()));

        msg.body
            .set_groups(fix44::NO_PARTY_I_DS.tag(), vec![party_1, party_2]);
        let config = Config { separator: b'|' };
        let raw_message = msg.encode(&config);

        let dict = Dictionary::fix44();
        let parsed_message = Message::from_bytes(config, &dict, &raw_message);

        let party_a = parsed_message.get_group(fix44::NO_PARTY_I_DS, 0).unwrap();
        let party_a_0 = party_a
            .get_group(fix44::NO_PARTY_SUB_I_DS.tag(), 0)
            .unwrap();
        let sub_id_0 = party_a_0.get_raw(fix44::PARTY_SUB_ID.tag()).unwrap();
        assert_eq!(sub_id_0.data, b"SUBPARTY_A_1");

        let party_b = parsed_message.get_group(fix44::NO_PARTY_I_DS, 1).unwrap();
        let party_b_id = party_b.get_raw(fix44::PARTY_ID.tag()).unwrap();
        assert_eq!(party_b_id.data, b"PARTY_B");

        let party_b_role = party_b.get_raw(fix44::PARTY_ROLE.tag()).unwrap();
        assert_eq!(party_b_role.data, b"2");

        let checksum = parsed_message.get_raw(fix44::CHECK_SUM).unwrap();
        assert_eq!(checksum, b"036");

        let qty = parsed_message.get_raw(fix44::BODY_LENGTH).unwrap();
        assert_eq!(qty, b"253");
    }
}
