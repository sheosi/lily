use std::fmt::{self, Debug};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

#[cfg(feature="python_skills")]
use crate::python::try_translate_all;
use crate::vars::POISON_MSG;

use serde::{Deserialize, Deserializer, de::{self, SeqAccess, Visitor}, Serialize, Serializer, ser::SerializeSeq};
use thiserror::Error;
use unic_langid::LanguageIdentifier;

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

#[derive(Clone, Debug)]
pub struct StringList {
    pub data: Vec<String>
}

impl StringList {
    pub fn new() -> Self {
        Self{data: Vec::new()}
    }
    pub fn from_vec(vec: Vec<String>) -> Self {
        Self{ data: vec}
    }

    #[cfg(feature="python_skills")]
    /// Returns an aggregated vector with the translations of all entries
    pub fn into_translation(self, lang: &LanguageIdentifier) -> Result<Vec<String>,Vec<String>> {
        let lang_str = lang.to_string();

        let (utts_data, failed):(Vec<_>,Vec<_>) = self.data.into_iter()
        .map(|utt|try_translate_all(&utt, &lang_str))
        .partition(Result::is_ok);

        if failed.is_empty() {
            let utts = utts_data.into_iter().map(Result::unwrap)
            .flatten().collect();

            Ok(utts)
        }
        else {
            let failed = failed.into_iter().map(Result::unwrap)
            .flatten().collect();
            Err(failed)            
        }
    }
    #[cfg(feature="python_skills")]
    /// Returns an aggregated vector with the translations of all entries
    pub fn to_translation(&self, lang: &LanguageIdentifier) -> Result<Vec<String>,Vec<String>> {
        let lang_str = lang.to_string();

        let (utts_data, failed):(Vec<_>,Vec<_>) = self.data.iter()
        .map(|utt|try_translate_all(&utt, &lang_str))
        .partition(Result::is_ok);

        if failed.is_empty() {
            let utts = utts_data.into_iter().map(Result::unwrap)
            .flatten().collect();

            Ok(utts)
        }
        else {
            let failed = failed.into_iter().map(Result::unwrap)
            .flatten().collect();
            Err(failed)            
        }
    }
}

struct StringListVisitor;

impl<'de> Visitor<'de> for StringListVisitor {
    type Value = StringList;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("either a string or a list containing strings")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> where E: de::Error{
        Ok(StringList{data:vec![v.to_string()]})
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E> where E: de::Error{
        Ok(StringList{data:vec![v]})
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
        let mut res = StringList{data: Vec::with_capacity(seq.size_hint().unwrap_or(1))};
        loop {
            match seq.next_element()? {
                Some(val) => {res.data.push(val)},
                None => {break}
            }
        }

        return Ok(res);   
    }
}

impl<'de> Deserialize<'de> for StringList {
    fn deserialize<D>(deserializer: D) -> Result<StringList, D::Error>
    where D: Deserializer<'de> {
        deserializer.deserialize_any(StringListVisitor)
    }
}

// Serialize this as a list of strings
impl Serialize for StringList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,{
        let mut seq = serializer.serialize_seq(Some(self.data.len()))?;
        for e in &self.data {
            seq.serialize_element(e)?;
        }
        seq.end()
    }
}
