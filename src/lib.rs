#![forbid(unsafe_code)]

pub mod codec;
pub mod codegen;
pub mod data;
pub mod error;
pub mod generators;
pub mod http;
pub mod identifiers;
pub mod io;
pub mod security;
pub mod sql;
pub mod text;
pub mod time;

pub use error::{Result, VutilsError};
