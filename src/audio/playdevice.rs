use std::time::Duration;
use std::thread::sleep;
use crate::audio::{Audio, Data};
use rodio::{source::Source, decoder::DecoderError, OutputStream, OutputStreamHandle};
use thiserror::Error;

pub struct PlayDevice {
    _stream: OutputStream, // We need to preserve this
    stream_handle: OutputStreamHandle
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
        let (_stream, stream_handle) = rodio::OutputStream::try_default().unwrap();
        
        Some(PlayDevice {_stream, stream_handle})
    }
    
    pub fn play_file(&mut self, path: &str) -> Result<(), PlayFileError> {
        /*let file = std::fs::File::open(path)?;
        let source = rodio::Decoder::new(std::io::BufReader::new(file))?;
        self.stream_handle.play_raw(source.convert_samples()).unwrap();*/
        std::process::Command::new("/usr/bin/ogg123").args(&["-q",path]).status().unwrap();

        Ok(())
    }

    pub fn play_audio(&mut self, audio: Audio) -> Result<(), DecoderError> {
        match audio.buffer {
            Data::Raw(raw_data) => {
                let source = rodio::buffer::SamplesBuffer::new(1, audio.samples_per_second, raw_data);
                self.stream_handle.play_raw(source.convert_samples()).unwrap();

                Ok(())
            },
            Data::Encoded(enc_data) => {
                let source = rodio::Decoder::new(std::io::Cursor::new(enc_data))?;
                self.stream_handle.play_raw(source.convert_samples()).unwrap();

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