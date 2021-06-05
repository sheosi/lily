use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::vars::POISON_MSG;

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