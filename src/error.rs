use std::{io, path::PathBuf};

use thiserror::Error;

pub type Result<T> = std::result::Result<T, VutilsError>;

#[derive(Debug, Error)]
pub enum VutilsError {
    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("unsupported operation: {0}")]
    Unsupported(String),

    #[error("failed to read `{path}`: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("failed to write `{path}`: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("{0}")]
    Message(String),
}

impl From<io::Error> for VutilsError {
    fn from(value: io::Error) -> Self {
        Self::Message(value.to_string())
    }
}
