use std::time::{SystemTime, Duration, UNIX_EPOCH};
use std::convert::TryInto;

use crate::vars::CLOCK_TOO_EARLY_MSG;

use hound;
use rodio::source::Source;
use thiserror::Error;

#[cfg(feature = "devel_cpal_rec")]
use cpal::traits::HostTrait;

pub struct PlayDevice {
    device: rodio::Device
}


pub enum PlayFileError {
    IoErr(std::io::Error),
    DecoderError(rodio::decoder::DecoderError)
}

impl std::convert::From<std::io::Error> for PlayFileError {
    fn from(err: std::io::Error) -> PlayFileError {
        PlayFileError::IoErr(err)
    }
}

impl std::convert::From<rodio::decoder::DecoderError> for PlayFileError {
    fn from(err: rodio::decoder::DecoderError) -> PlayFileError {
        PlayFileError::DecoderError(err)
    }
}

impl PlayDevice {
    pub fn new() -> Option<PlayDevice> {
        let device = rodio::default_output_device()?;
        
        Some(PlayDevice {device})
    }
    
    pub fn play_file(&mut self, path: &str) -> Result<(), PlayFileError> {
        let file = std::fs::File::open(path)?;
        let source = rodio::Decoder::new(std::io::BufReader::new(file))?;
        rodio::play_raw(&self.device, source.convert_samples());

        Ok(())
    }

    pub fn play_audio(&mut self, audio: Audio) {
        match audio.buffer {
            Data::Raw(raw_data) => {
                let source = rodio::buffer::SamplesBuffer::new(1, audio.samples_per_second, raw_data);
                rodio::play_raw(&self.device, source.convert_samples());
            },
            Data::Encoded(enc_data) => {
                let source = rodio::Decoder::new(std::io::Cursor::new(enc_data)).unwrap();
                rodio::play_raw(&self.device, source.convert_samples());

            }
        }   
    }
}

pub struct RecDevice {
    device: sphinxad::AudioDevice,
    buffer: [i16; 4096],
    last_read: u128
}

pub trait Recording {
    fn read(&mut self) -> Result<Option<&[i16]>, std::io::Error>;
    fn read_for_ms(&mut self, milis: u16) -> Result<Option<&[i16]>, std::io::Error>;
    fn start_recording(&mut self) -> Result<(), std::io::Error>;
    fn stop_recording(&mut self) -> Result<(), std::io::Error>;
}

impl RecDevice {
    pub fn new() -> Result<RecDevice, std::io::Error> {
        //let host = cpal::default_host();
        //let device = host.default_input_device().expect("Something failed");

        let device = sphinxad::AudioDevice::default_with_sps(16000)?;

        Ok(RecDevice {
            device,
            buffer: [0i16; 4096],
            last_read: 0
        })

    }

    fn get_millis() -> u128 {
        SystemTime::now().duration_since(UNIX_EPOCH).expect(CLOCK_TOO_EARLY_MSG).as_millis()
    }
}

impl Recording for RecDevice {
    fn read(&mut self) -> Result<Option<&[i16]>, std::io::Error> {
        self.last_read = Self::get_millis();
        self.device.read(&mut self.buffer[..])
    }

    fn read_for_ms(&mut self, milis: u16) -> Result<Option<&[i16]>, std::io::Error> {
        let curr_time = Self::get_millis();
        let diff_time = (curr_time - self.last_read) as u16;
        if milis > diff_time{
            let sleep_time = (milis  - diff_time) as u64 ;
            std::thread::sleep(Duration::from_millis(sleep_time));
        }
        else {
            //log::info!("We took {}ms more from what we were asked ({})", diff_time - milis, milis);
        }
        
        self.read()
    }

    fn start_recording(&mut self) -> Result<(), std::io::Error> {
        self.last_read = SystemTime::now().duration_since(UNIX_EPOCH).expect(CLOCK_TOO_EARLY_MSG).as_millis();
        self.device.start_recording()
    }
    fn stop_recording(&mut self) -> Result<(), std::io::Error> {
        self.device.stop_recording()
    }
}

#[cfg(feature = "devel_cpal_rec")]
pub struct RecDeviceCpal {
    device: cpal::Device,
    buffer: [i16; 2048],
}

#[cfg(feature = "devel_cpal_rec")]
impl RecDeviceCpal {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let device = host.default_input_device().expect("Something failed");
        //let format = 

        RecDeviceCpal {
            device,
            buffer: [0i16; 2048],
        }

    }
}

#[cfg(feature = "devel_cpal_rec")]
impl Recording for RecDeviceCpal {
    fn read(&mut self) -> Option<&[i16]> {
        None
        // NYI
        //self.device.read(&mut self.buffer[..]).unwrap()
    }
    fn read_for_ms(&mut self, milis: u16) -> Option<&[i16]> {
        None
    }

    fn start_recording(&mut self) -> Result<(), std::io::Error> {
        //self.device.start_recording()   
        // NYI
        Ok(())
    }
    fn stop_recording(&mut self) -> Result<(), std::io::Error> {
        //self.device.stop_recording()
        // NYI
        Ok(())
    }
}

enum Data {
    Raw(Vec<i16>),
    Encoded(Vec<u8>)
}

impl Data {

    fn clear(&mut self) {
        match self {
            Data::Raw(raw_data) => raw_data.clear(),
            Data::Encoded(enc_data) => enc_data.clear()
        }
    }

    fn append_raw(&mut self, b: &[i16]) {
        match self {
            Data::Raw(data_self) => data_self.extend(b),
            Data::Encoded(_) => std::panic!("Tried to append a raw audio to an encoded audio")
        }
    }

    fn is_raw(&self) -> bool {
        match self {
            Data::Raw(_) => true,
            Data::Encoded(_) => false
        }
    }

    fn use_writer<T: std::io::Write + std::io::Seek>(&self, writer: &mut hound::WavWriter<T>) -> Result<(), hound::Error> {
        match self {
            Data::Raw(data) =>  {
                for i in 0 .. data.len() {
                    writer.write_sample(data[i])?;
                }

                Ok(())
            },
            Data::Encoded(_) => panic!("Can't write to wav an encoded audio")
        }
    }
}

// Just some and audio dummy for now
pub struct Audio {
    buffer: Data,
    pub samples_per_second: u32
}

impl Audio {
    pub fn new_empty(samples_per_second: u32) -> Self {
        Self{buffer: Data::Raw(Vec::new()), samples_per_second}
    }

    pub fn new_raw(buffer: Vec<i16>, samples_per_second: u32) -> Self {
        Self {buffer: Data::Raw(buffer), samples_per_second}
    }

    pub fn new_encoded(buffer: Vec<u8>, samples_per_second: u32) -> Self {
        Self {buffer: Data::Encoded(buffer), samples_per_second}
    }


    pub fn append_raw(&mut self, other: &[i16], samples_per_second: u32) -> Option<()> {
        if self.samples_per_second == samples_per_second && self.buffer.is_raw() {
            self.buffer.append_raw(other);
            Some(())
        }
        else {
            // Can't join if it's not the same sample rate
            None
        }
    }

    pub fn write_wav(&self, filename:&str) -> Result<(), WavError> {

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.samples_per_second,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(filename, spec)?;
        self.buffer.use_writer(&mut writer).unwrap();

        

        Ok(())
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}


// For managing Audio only in Raw, much more simple and a little more focused on speed
pub struct AudioRaw {
    pub buffer: Vec<i16>,
    pub samples_per_second: u32
}

impl AudioRaw {
    pub fn new_empty(samples_per_second: u32) -> Self {
        AudioRaw{buffer: Vec:: new(), samples_per_second}
    }

    pub fn new_raw(buffer: Vec<i16>, samples_per_second: u32) -> Self {
        AudioRaw{buffer, samples_per_second}
    }

    pub fn clear(&mut self) {
        self.buffer.clear()
    }

    pub fn append_audio(&mut self, other: &[i16], sps: u32) -> bool {
        if self.samples_per_second == sps {
            self.buffer.extend(other);

            true
        }
        else {
            false
        }
    }


    pub fn to_wav(&self) -> Result<Vec<u8>, WavError> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.samples_per_second,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut buffer: Vec<u8> = Vec::new();
        {
            let cursor = std::io::Cursor::new(&mut buffer);
            let mut writer = hound::WavWriter::new(cursor,spec)?;
            let mut sample_writer = writer.get_i16_writer(self.buffer.len().try_into().map_err(|_|WavError::TooBig)?);

            for i in 0 .. self.buffer.len() {
                sample_writer.write_sample(self.buffer[i]);
            }

            sample_writer.flush()?;
        }

        Ok(buffer)
    }

}

#[derive(Error, Debug)]
pub enum WavError {
    #[error("hound error")]
    Hound(#[from] hound::Error),

    #[error("this buffer size is too big ")]
    TooBig
}