use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::vars::POISON_MSG;

use thiserror::Error;
use unic_langid::{LanguageIdentifier, LanguageIdentifierError};

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

pub trait LockIt<T: ?std::marker::Sized> {
    fn lock_it(&self) -> MutexGuard<T>;
}

impl<T: ?std::marker::Sized> LockIt<T> for &Arc<Mutex<T>> {
    fn lock_it(&self) -> MutexGuard<T> {
        self.lock().expect(POISON_MSG)
    }
}

impl<T: ?std::marker::Sized> LockIt<T> for Mutex<T> {
    fn lock_it(&self) -> MutexGuard<T> {
        self.lock().expect(POISON_MSG)
    }
}

/// Transform all languages into their LanguageIdentifier forms
fn parse_langs(langs: Vec<String>) -> Result<Vec<LanguageIdentifier>, LanguageIdentifierError> {
    Ok(langs
        .into_iter()
        .map(|l|l.parse())
        .collect::<Result<Vec<_>,_>>()?)
}