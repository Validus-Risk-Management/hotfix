use crate::error::HeaderParsingError;
use crate::message::Message;

pub enum ParsedMessage {
    Valid(Message),
    Invalid {
        message: Message,
        reason: InvalidReason,
    },
    Garbled(GarbledReason),
    UnexpectedError(String),
}

impl ParsedMessage {
    pub fn into_message(self) -> Option<Message> {
        match self {
            ParsedMessage::Valid(message) => Some(message),
            ParsedMessage::Invalid { message, .. } => Some(message),
            _ => None,
        }
    }
}

pub enum InvalidReason {
    InvalidField(u32),
    InvalidGroup(u32),
    InvalidComponent(String),
}

pub enum GarbledReason {
    Malformed,
    InvalidBeginString,
    InvalidBodyLength,
    InvalidMsgType,
    InvalidChecksum,
}

impl From<HeaderParsingError> for ParsedMessage {
    fn from(header_error: HeaderParsingError) -> Self {
        match header_error {
            HeaderParsingError::InvalidBeginString => {
                ParsedMessage::Garbled(GarbledReason::InvalidBeginString)
            }
            HeaderParsingError::InvalidBodyLength => {
                ParsedMessage::Garbled(GarbledReason::InvalidBodyLength)
            }
            HeaderParsingError::InvalidMsgType => {
                ParsedMessage::Garbled(GarbledReason::InvalidMsgType)
            }
            HeaderParsingError::IncompleteMessage => {
                ParsedMessage::Garbled(GarbledReason::Malformed)
            }
        }
    }
}
