use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error")]
    IOError(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
