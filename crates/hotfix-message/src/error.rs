use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EncodingError {
    #[error("IO error")]
    IOError(#[from] io::Error),
}

pub type EncodingResult<T> = Result<T, EncodingError>;

#[derive(Error, Debug)]
pub enum ParserError {
    #[error("IO error")]
    IOError(#[from] io::Error),
    #[error("field (tag = {0}) is missing from FIX dictionary")]
    InvalidField(u32),
    #[error("group (tag = {0}) is missing from FIX dictionary")]
    InvalidGroup(u32),
    #[error("component (name = {0}) is missing from FIX dictionary")]
    InvalidComponent(String),
    #[error("malformed message: {0}")]
    Malformed(String),
}

pub type ParserResult<T> = Result<T, ParserError>;

#[derive(Error, Debug)]
pub(crate) enum HeaderParsingError {
    #[error("Invalid BeginString")]
    InvalidBeginString,
    #[error("Invalid BodyLength")]
    InvalidBodyLength,
    #[error("Invalid MsgType")]
    InvalidMsgType,
    #[error("Message ended before reaching body")]
    IncompleteMessage,
}

pub(crate) type HeaderParsingResult<T> = Result<T, HeaderParsingError>;

#[derive(Error, Debug)]
pub(crate) enum TrailerParsingError {
    #[error("Invalid CheckSum")]
    InvalidCheckSum,
    #[error("Invalid field")]
    InvalidField(u32),
}

pub(crate) type TrailerParsingResult<T> = Result<T, TrailerParsingError>;
