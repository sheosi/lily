use std::time::{SystemTime, Duration, UNIX_EPOCH};
use crate::vars::{CLOCK_TOO_EARLY_MSG, DEFAULT_SAMPLES_PER_SECOND, RECORD_BUFFER_SIZE};
use log::info;
use thiserror::Error;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleRate, Stream, StreamConfig};
use ringbuf::{Consumer, RingBuffer};
use log::error;

#[derive(Error, Debug)]
pub enum RecordingError {
    #[error("Failed to do I/O operations")]
    IoError(#[from]std::io::Error),

    #[error("No input device")]
    NoInputDevice,

    #[error("Couldn't build stream")]
    BuildStream(#[from] cpal::BuildStreamError),

    #[error("Failed to play the stream")]
    PlayStreamError(#[from] cpal::PlayStreamError)
}

pub trait Recording {
    fn read(&mut self) -> Result<Option<&[i16]>, RecordingError>;
    fn read_for_ms(&mut self, milis: u16) -> Result<Option<&[i16]>, RecordingError>;
    fn start_recording(&mut self) -> Result<(), RecordingError>;
    fn stop_recording(&mut self) -> Result<(), RecordingError>;
}

// Cpal version
pub struct RecDevice {
    external_buffer: [i16; RECORD_BUFFER_SIZE],
    stream_data: Option<StreamData>
}

struct StreamData {
    internal_buffer_consumer: Consumer<i16>,
    last_read: u128,
    _stream: Stream
}

impl RecDevice {
    // For now just use that error to original RecDevice
    pub fn new() -> Result<Self, RecordingError> {

        Ok(RecDevice {
            external_buffer: [0i16; RECORD_BUFFER_SIZE],
            stream_data: None
        })

    }

    fn make_stream() -> Result<(Stream, Consumer<i16>), RecordingError> {
        info!("Using cpal");
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(RecordingError::NoInputDevice)?;
        // TODO: Make sure audio is compatible with our application and/or negotiate
        // device.default_input_config()?;
        let config = StreamConfig {
            channels: 1,
            sample_rate: SampleRate(DEFAULT_SAMPLES_PER_SECOND),
            buffer_size: BufferSize::Default
        };

        let internal_buffer = RingBuffer::new(RECORD_BUFFER_SIZE * 2);
        let (mut prod, cons) = internal_buffer.split();

        let err_fn = move |err| {
            error!("An error ocurred on stream: {}", err);
        };

        let stream = device.build_input_stream(
            &config,
            move |data: &[i16], _: &_| {
                prod.push_slice(&data);
            },
            err_fn
        )?;

        // Do make sure stream is working
        stream.play()?;

        Ok((stream, cons))
    }

    fn get_millis() -> u128 {
        SystemTime::now().duration_since(UNIX_EPOCH).expect(CLOCK_TOO_EARLY_MSG).as_millis()
    }
}

impl Recording for RecDevice {
    fn read(&mut self) -> Result<Option<&[i16]>, RecordingError> {
        match self.stream_data {
            Some(ref mut str_data) => {
                str_data.last_read = Self::get_millis();
                let size = str_data.internal_buffer_consumer.pop_slice(&mut self.external_buffer[..]);
                if size > 0 {
                    Ok(Some(&self.external_buffer[0..size]))
                }
                else {
                    Ok(None)
                }
            },
            None => {
                panic!("Read called when a recdevice was stopped");
            }
        }
        
        
    }
    fn read_for_ms(&mut self, milis: u16) -> Result<Option<&[i16]>, RecordingError> {
        match self.stream_data {
            Some(ref mut str_data) => {
                let curr_time = Self::get_millis();
                let diff_time = (curr_time - str_data.last_read) as u16;
                if milis > diff_time{
                    let sleep_time = (milis  - diff_time) as u64 ;
                    std::thread::sleep(Duration::from_millis(sleep_time));
                }
            },
            None => {
                panic!("read_for_ms called when a recdevice was stopped");
            }
        }

        self.read()
    }

    fn start_recording(&mut self) -> Result<(), RecordingError> {
        let (_stream, consumer) = Self::make_stream()?;
        self.stream_data = Some(StreamData {
            internal_buffer_consumer: consumer,
            last_read: 0u128,
            _stream
        });

        Ok(())
    }
    fn stop_recording(&mut self) -> Result<(), RecordingError> {
        self.stream_data = None;
        Ok(())
    }
}