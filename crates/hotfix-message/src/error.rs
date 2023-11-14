use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error")]
    IOError(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum ParserError {
    #[error("IO error")]
    IOError(#[from] io::Error),
    #[error("field (tag = {0}) is missing from FIX dictionary")]
    InvalidField(u32),
}

pub type ParserResult<T> = std::result::Result<T, ParserError>;
