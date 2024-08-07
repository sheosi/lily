use std::io::Cursor;
use std::time::Duration;

use crate::audio::{Audio, AudioRaw, Data};
use crate::vars::MAX_SAMPLES_PER_SECOND;

use ogg_opus::decode;
use rodio::{source::Source, OutputStream, OutputStreamHandle, StreamError};
use thiserror::Error;
use tokio::time::sleep;

pub struct PlayDevice {
    _stream: OutputStream, // We need to preserve this
    stream_handle: OutputStreamHandle,
}

#[derive(Error, Debug)]
pub enum PlayAudioError {
    #[error("Failed while doing IO")]
    IoErr(#[from] std::io::Error),
    #[error("Failed while decoding")]
    DecoderError(#[from] rodio::decoder::DecoderError),
    #[error("Couldn't play audio, reason: {}", .0)]
    PlayError(String),
    #[error("Coudln't transform audio")]
    TransformationError(#[from] ogg_opus::Error),
}

impl From<rodio::PlayError> for PlayAudioError {
    fn from(err: rodio::PlayError) -> Self {
        PlayAudioError::PlayError(format!("{:?}", err))
    }
}

impl PlayDevice {
    pub fn new() -> Result<PlayDevice, StreamError> {
        let (_stream, stream_handle) = rodio::OutputStream::try_default()?;

        Ok(PlayDevice {
            _stream,
            stream_handle,
        })
    }

    pub fn play_file(&mut self, path: &str) -> Result<(), PlayAudioError> {
        let file = std::fs::File::open(path)?;
        let source = rodio::Decoder::new(std::io::BufReader::new(file))?;

        self.stream_handle.play_raw(source.convert_samples())?;

        Ok(())
    }

    pub fn play_audio(&mut self, audio: Audio) -> Result<(), PlayAudioError> {
        match audio.buffer {
            Data::Raw(raw_data) => {
                let source = rodio::buffer::SamplesBuffer::new(
                    1,
                    AudioRaw::get_samples_per_second(),
                    raw_data.buffer,
                );
                self.stream_handle.play_raw(source.convert_samples())?;
            }
            Data::Encoded(enc_data) => {
                if enc_data.is_ogg_opus() {
                    let (audio, play_data) =
                        decode::<_, MAX_SAMPLES_PER_SECOND>(Cursor::new(enc_data.data))?;
                    let source = rodio::buffer::SamplesBuffer::new(
                        play_data.channels,
                        MAX_SAMPLES_PER_SECOND,
                        audio,
                    );
                    self.stream_handle.play_raw(source.convert_samples())?;
                } else {
                    let source = rodio::Decoder::new(std::io::Cursor::new(enc_data.data))?;
                    self.stream_handle.play_raw(source.convert_samples())?;
                }
            }
        }
        Ok(())
    }

    pub async fn wait_audio(&mut self, audio: Audio) -> Result<(), PlayAudioError> {
        let seconds = audio.len_s();
        self.play_audio(audio)?;
        let ms_wait = (seconds * 1000.0).ceil() as u64;
        sleep(Duration::from_millis(ms_wait)).await;

        Ok(())
    }
}
