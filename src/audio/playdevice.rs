use std::time::Duration;
use std::thread::sleep;
use crate::audio::{Audio, Data};
use rodio::{source::Source, decoder::DecoderError};
use thiserror::Error;

pub struct PlayDevice {
    device: rodio::Device
}

#[derive(Error, Debug)]
pub enum PlayFileError {
    #[error("Failed while doing IO")]
    IoErr(#[from] std::io::Error),
    #[error("Failed while decoding")]
    DecoderError(#[from] rodio::decoder::DecoderError)
}

impl PlayDevice  {
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

    pub fn play_audio(&mut self, audio: Audio) -> Result<(), DecoderError> {
        match audio.buffer {
            Data::Raw(raw_data) => {
                let source = rodio::buffer::SamplesBuffer::new(1, audio.samples_per_second, raw_data);
                rodio::play_raw(&self.device, source.convert_samples());
                Ok(())
            },
            Data::Encoded(enc_data) => {
                let source = rodio::Decoder::new(std::io::Cursor::new(enc_data))?;
                rodio::play_raw(&self.device, source.convert_samples());
                Ok(())
            }
        }   
    }

    pub fn wait_audio(&mut self, audio: Audio) -> Result<(), DecoderError> {
        let seconds = audio.len_s();
        self.play_audio(audio)?;
        let ms_wait = (seconds * 1000.0).ceil() as u64;
        sleep(Duration::from_millis(ms_wait));

        Ok(())
    }
}