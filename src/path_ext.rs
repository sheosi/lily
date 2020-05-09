use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
#[error("\"{}\" contains not-unicode characters", debug_str)]
pub struct NotUnicodeError {
    debug_str: String
}

pub trait ToStrResult {
    fn to_str_res(&self) -> Result<&str, NotUnicodeError>;
}

impl ToStrResult for PathBuf {
    fn to_str_res(&self) -> Result<&str, NotUnicodeError> {
        self.to_str().ok_or_else(|| NotUnicodeError{debug_str: format!("{:?}", self)})
    }
}