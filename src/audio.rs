use std::time::{SystemTime, Duration, UNIX_EPOCH};
use std::convert::TryInto;

use crate::vars::CLOCK_TOO_EARLY_MSG;

use hound;
use rodio::source::Source;

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
    
    pub fn play(&mut self, buf: &[i16], samples: u32) {
        let source = rodio::buffer::SamplesBuffer::new(1, samples, buf);
        rodio::play_raw(&self.device, source.convert_samples());
    }
    
    pub fn play_file(&mut self, path: &str) -> Result<(), PlayFileError> {
        let file = std::fs::File::open(path)?;
        let source = rodio::Decoder::new(std::io::BufReader::new(file))?;
        rodio::play_raw(&self.device, source.convert_samples());

        Ok(())
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

// Just some and audio dummy for now
pub struct Audio {
    pub buffer: Vec<i16>,
    pub samples_per_second: u32
}

impl Audio {
    pub fn new_empty(samples_per_second: u32) -> Self {
        Self{buffer: Vec::new(), samples_per_second}
    }

    pub fn join(&self, other: &Audio) -> Option<Audio> {
        if self.samples_per_second == other.samples_per_second {
            let new_buffer = [&self.buffer[..], &other.buffer[..]].concat();

            Some(Audio{buffer: new_buffer, samples_per_second: self.samples_per_second})
        }
        else {
            // Can't join if it's not the same sample rate
            None
        }
    }

    pub fn append(&mut self, other: &Audio) -> Option<()> {
        if self.samples_per_second == other.samples_per_second {
            self.buffer.extend(&other.buffer);
            Some(())
        }
        else {
            // Can't join if it's not the same sample rate
            None
        }
    }

    pub fn append_audio(&mut self, other: &[i16], samples_per_second: u32) -> Option<()> {
        if self.samples_per_second == samples_per_second {
            self.buffer.extend(other);
            Some(())
        }
        else {
            // Can't join if it's not the same sample rate
            None
        }
    }

    pub fn write_wav(&self, filename:&str) -> Result<(), hound::Error> {

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.samples_per_second,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(filename, spec)?;
        for i in 0 .. self.buffer.len() {
            writer.write_sample(self.buffer[i])?;
        }

        Ok(())
    }

    pub fn to_wav(&self) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.samples_per_second,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut buffer: Vec<u8> = Vec::new();
        {
            let cursor = std::io::Cursor::new(&mut buffer);
            let mut writer = hound::WavWriter::new(cursor,spec).unwrap();
            let mut sample_writer = writer.get_i16_writer(self.buffer.len().try_into().unwrap());

            for i in 0 .. self.buffer.len() {
                sample_writer.write_sample(self.buffer[i]);
            }

            sample_writer.flush().unwrap();
        }

        buffer
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

