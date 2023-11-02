use hotfix_dictionary::Dictionary;
use std::slice::Iter;

use crate::field_map::FieldMap;

#[derive(Default)]
struct Header {
    fields: FieldMap,
}

#[derive(Default)]
struct Trailer {
    fields: FieldMap,
}

#[derive(Default)]
struct Message {
    header: Header,
    trailer: Trailer,
}

impl Message {
    fn from_bytes(dict: &Dictionary, data: &[u8]) -> Self {
        message_from_bytes(dict, data)
    }
}

fn message_from_bytes(_dict: &Dictionary, data: &[u8]) -> Message {
    let _stream = data.iter();
    todo!()
}

fn header_from_bytes(_dict: &Dictionary, _stream: &mut Iter<u8>) -> Header {
    // first three fields need to be BeginString (8), BodyLength (9), and MsgType(35)
    // https://www.onixs.biz/fix-dictionary/4.4/compblock_standardheader.html
    let header = Header::default();

    header
}
