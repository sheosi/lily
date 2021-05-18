#[cfg(feature = "client")]
mod playdevice;
#[cfg(feature = "client")]
mod recdevice;

#[cfg(feature = "client")]
pub use self::playdevice::*;
#[cfg(feature = "client")]
pub use self::recdevice::*;

use std::io::{Cursor, Write};
use std::path::Path;
use crate::vars::DEFAULT_SAMPLES_PER_SECOND;

use log::warn;
use ogg_opus::encode;
use thiserror::Error;

#[derive(Debug, Clone)]
struct AudioEncoded {
    data: Vec<u8>
}

impl AudioEncoded {
    fn new(data: Vec<u8>) -> Self {
        Self {data}
    }

    #[cfg(feature="client")]
    pub fn is_ogg_opus(&self) -> bool {
        ogg_opus::is_ogg_opus(Cursor::new(&self.data))
    }

    pub fn get_sps(&self) -> u32 {
        // Just some value, not yet implemented
        // TODO: Finish it
        warn!("AudioEncoded::get_sps not yet implemented");
        48000
    }
}

#[derive(Debug, Clone)]
enum Data {
    Raw(AudioRaw),
    Encoded(AudioEncoded)
}

impl Data {

    fn clear(&mut self) {
        match self {
            Data::Raw(raw_data) => raw_data.clear(),
            Data::Encoded(enc_data) => enc_data.data.clear()
        }
    }

    fn append_raw(&mut self, b: &[i16]) {
        match self {
            Data::Raw(data_self) => data_self.append_audio(b, DEFAULT_SAMPLES_PER_SECOND).expect("Tried to append but one of the raw data wasn't using default sps"),
            Data::Encoded(_) => std::panic!("Tried to append a raw audio to an encoded audio")
        }
    }

    fn is_raw(&self) -> bool {
        match self {
            Data::Raw(_) => true,
            Data::Encoded(_) => false
        }
    }

    fn len(&self) -> usize {
        match self {
            Data::Raw(buffer) => buffer.len(),
            Data::Encoded(buffer) => buffer.data.len()
        }
    }

    fn get_sps(&self) -> u32 {
        match self {
            Data::Raw(_) => DEFAULT_SAMPLES_PER_SECOND,
            Data::Encoded(data) => data.get_sps()
        }
    }
}

// Just some and audio dummy for now
#[derive(Debug, Clone)]
pub struct Audio {
    buffer: Data
}

impl Audio {
    pub fn new_empty(samples_per_second: u32) -> Self {
        assert_eq!(samples_per_second, DEFAULT_SAMPLES_PER_SECOND);
        Self{buffer: Data::Raw(AudioRaw::new_empty(samples_per_second))}
    }

    pub fn new_raw(buffer: Vec<i16>, samples_per_second: u32) -> Self {
        assert_eq!(samples_per_second, DEFAULT_SAMPLES_PER_SECOND);
        Self {buffer: Data::Raw(AudioRaw::new_raw(buffer, samples_per_second))}
    }

    pub fn new_encoded(buffer: Vec<u8>) -> Self {
        Self {buffer: Data::Encoded(AudioEncoded::new(buffer))}
    }


    pub fn append_raw(&mut self, other: &[i16], samples_per_second: u32) -> Option<()> {
        if self.buffer.is_raw() {
            assert_eq!(samples_per_second, DEFAULT_SAMPLES_PER_SECOND);
            self.buffer.append_raw(other);
            Some(())
        }
        else {
            // Can't join if it's not the same sample rate
            None
        }
    }

    pub fn write_ogg(&self, file_path:&Path) -> Result<(), AudioError> {

        match &self.buffer {
            Data::Raw(audio_raw) => {
                let as_ogg = audio_raw.to_ogg_opus()?;
                let mut file = std::fs::File::create(file_path)?;
                file.write_all(&as_ogg)?;
            }
            Data::Encoded(vec_data) => {
                let mut file = std::fs::File::create(file_path)?;
                file.write_all(&vec_data.data)?;
            }
        }

        

        Ok(())
    }

    pub fn into_encoded(self) -> Result<Vec<u8>, AudioError> {
        match self.buffer {
            Data::Raw(audio_raw) => {
                audio_raw.to_ogg_opus()
            }
            Data::Encoded(vec_data) => {Ok(vec_data.data)}
        }
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    // Length in seconds
    pub fn len_s(&self) -> f32 {
        let len = self.buffer.len();
        (len as f32)/(self.buffer.get_sps() as f32)
    }

    pub fn from_raw(raw: AudioRaw) -> Self {
        Self{buffer: Data::Raw(raw)}
    }
}

// For managing raw audio, mostly coming from the mic,
// is fixed at 16 KHz and mono (what most STTs )
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioRaw {
    pub buffer: Vec<i16>
}

impl AudioRaw {
    pub fn get_samples_per_second() -> u32 {
        DEFAULT_SAMPLES_PER_SECOND
    }
    pub fn new_empty(samples_per_second: u32) -> Self {
        assert!(samples_per_second == Self::get_samples_per_second());
        AudioRaw{buffer: Vec:: new()}
    }

    pub fn new_raw(buffer: Vec<i16>, samples_per_second: u32) -> Self {
        assert!(samples_per_second == Self::get_samples_per_second());
        AudioRaw{buffer}
    }

    pub fn clear(&mut self) {
        self.buffer.clear()
    }

    pub fn append_audio(&mut self, other: &[i16], sps: u32) -> Result<(), AudioError> {
        if sps == Self::get_samples_per_second() {
            self.buffer.extend(other);
            Ok(())
        }
        else {
            Err(AudioError::IncompatibleSps)
        }
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn rms(&self) -> f64 {
        let sqr_sum = self.buffer.iter().fold(0i64, |sqr_sum, s|{
            sqr_sum + (*s as i64)  * (*s as i64)
        });
        (sqr_sum as f64/ self.buffer.len() as f64).sqrt()

    }

    // Length in seconds
    pub fn len_s(&self) -> f32 {
        let len = self.buffer.len();
        (len as f32)/(Self::get_samples_per_second() as f32)
    }

    pub fn save_to_disk(&self, path: &Path) -> Result<(), AudioError> {
        let ogg = self.to_ogg_opus()?;
        let mut file = std::fs::File::create(path)?;
        file.write_all(&ogg)?;
        Ok(())
    }

    pub fn to_ogg_opus(&self) -> Result<Vec<u8>, AudioError> {
        Ok(encode::<16000, 1>(&self.buffer)?)
    }
}


#[derive(Error, Debug)]
pub enum AudioError {
    #[error("Io Error")]
    IOError(#[from] std::io::Error),

    #[error("Incompatible Samples per seconds")]
    IncompatibleSps,

    #[error("")]
    OggOpusError(#[from] ogg_opus::Error)
}