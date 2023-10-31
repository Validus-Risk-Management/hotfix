//! HeaderInfo taken from ferrumfix
use hotfix_encoding::dict::Dictionary;
use hotfix_encoding::Decoder;
use std::fmt::{Display, Formatter};
use std::ops::Range;

const FIELD_CHECKSUM_LEN_IN_BYTES: usize = 7; // the checksum is always 7 bytes

#[derive(Clone, Debug)]
pub struct RawFixMessage {
    data: Vec<u8>,
}

impl RawFixMessage {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

impl Display for RawFixMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let pretty_bytes: Vec<u8> = self
            .data
            .iter()
            .map(|b| if *b == b'\x01' { b'|' } else { *b })
            .collect();
        let s = std::str::from_utf8(&pretty_bytes).unwrap_or("invalid characters");

        write!(f, "{}", s)
    }
}

pub struct Parser {
    buffer: Vec<u8>,
    decoder: Decoder,
}

impl Parser {
    pub fn parse(&mut self, data: &[u8]) -> Vec<RawFixMessage> {
        let mut messages = vec![];
        self.buffer.extend_from_slice(data);
        while let Some(header_info) = HeaderInfo::parse(&self.buffer, b'\x01') {
            let message_length = header_info.message_length();
            if message_length > self.buffer.len() {
                break;
            }

            let (msg_data, remainder) = self.buffer.split_at(message_length + 1);
            let msg = self.decoder.decode(msg_data).expect("valid message");

            let raw_message = RawFixMessage {
                data: msg.as_bytes().to_vec(),
            };
            messages.push(raw_message);

            self.buffer = remainder.to_vec();
        }

        messages
    }
}

impl Default for Parser {
    fn default() -> Self {
        let decoder = Decoder::new(Dictionary::fix44());
        Self {
            buffer: vec![],
            decoder,
        }
    }
}

// TODO: this is a duplicate of HeaderInfo in hotfix-encoding, delete it
#[derive(Debug, Clone)]
pub struct HeaderInfo {
    field_0: Range<usize>,
    field_1: Range<usize>,
    nominal_body_len: usize,
}

impl HeaderInfo {
    fn parse(data: &[u8], separator: u8) -> Option<Self> {
        let mut info = Self {
            field_0: 0..1,
            field_1: 0..1,
            nominal_body_len: 0,
        };

        let mut iterator = data.iter();
        let mut find_byte = |byte| iterator.position(|b| *b == byte);
        let mut i = 0;

        i += find_byte(b'=')? + 1;
        info.field_0.start = i;
        i += find_byte(separator)?;
        info.field_0.end = i;
        i += 1;

        i += find_byte(b'=')? + 1;
        info.field_1.start = i;
        i += find_byte(separator)?;
        info.field_1.end = i;

        for byte in &data[info.field_1.clone()] {
            info.nominal_body_len = info
                .nominal_body_len
                .wrapping_mul(10)
                .wrapping_add(byte.wrapping_sub(b'0') as usize);
        }

        Some(info)
    }

    #[inline]
    fn message_length(&self) -> usize {
        self.field_1.end + self.nominal_body_len + FIELD_CHECKSUM_LEN_IN_BYTES
    }
}

#[cfg(test)]
mod tests {
    use crate::message::parser::{HeaderInfo, Parser};

    #[test]
    fn test_parsing_exact_message() {
        let data = b"8=FIX.4.4\x019=77\x0135=A\x0134=1\x0149=validus-fix\x0152=20230908-08:24:56.574\x0156=FXALL\x0198=0\x01108=30\x01141=Y\x0110=037\x01";
        let mut parser = Parser::default();

        let messages = parser.parse(data);
        assert_eq!(messages.len(), 1);
        assert_eq!(parser.buffer.len(), 0);
    }

    #[test]
    fn test_parsing_incomplete_message() {
        let data = b"8=FIX.4.4\x019=77\x0135=A\x0134=1\x0149=validus-fix\x0152=20230908-08:24:56.574\x0156=FXALL\x0198=0\x01108=30\x01141=Y";
        let mut parser = Parser::default();

        let messages = parser.parse(data);
        assert_eq!(messages.len(), 0);
        assert_eq!(parser.buffer.len(), data.len());
    }

    #[test]
    fn test_parsing_incomplete_message_then_completing() {
        // this isn't a complete message
        let data1 = b"8=FIX.4.4\x019=77\x0135=A\x0134=1\x0149=validus-fix\x0152=20230908-08:24:56.574\x0156=FXALL\x0198=0\x01108=30\x0114";
        // this contains the end of the previous message, plus a full new message
        let data2 = b"1=Y\x0110=037\x018=FIX.4.4\x019=77\x0135=A\x0134=2\x0149=validus-fix\x0152=20230908-08:24:58.574\x0156=FXALL\x0198=0\x01108=30\x01141=Y\x0110=040\x01";
        let mut parser = Parser::default();

        let messages = parser.parse(data1);
        assert_eq!(messages.len(), 0);
        assert_eq!(parser.buffer.len(), data1.len());

        let messages = parser.parse(data2);
        assert_eq!(messages.len(), 2);
        assert_eq!(parser.buffer.len(), 0);
    }

    #[test]
    fn test_incomplete_header_info() {
        let data = b"8=FIX.4.4\x019";
        let result = HeaderInfo::parse(data, b'\x01');

        assert!(result.is_none());
    }

    #[test]
    fn test_exact_header() {
        let data = b"8=FIX.4.4\x019=77\x01";
        let result = HeaderInfo::parse(data, b'\x01');

        assert!(result.is_some());
        assert_eq!(result.unwrap().nominal_body_len, 77);
    }

    #[test]
    fn test_full_message() {
        let data = b"8=FIX.4.4\x019=77\x0135=A\x0134=1\x0149=validus-fix\x0152=20230908-08:24:56.574\x0156=FXALL\x0198=0\x01108=30\x01141=Y\x0110=037\x01";
        let result = HeaderInfo::parse(data, b'\x01');

        assert!(result.is_some());
        assert_eq!(result.unwrap().nominal_body_len, 77);
    }
}
