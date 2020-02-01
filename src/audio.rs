use std::time::{SystemTime, Duration, UNIX_EPOCH};
use std::io::Write;


use crate::vars::{CLOCK_TOO_EARLY_MSG, LILY_VER};

use rodio::source::Source;
use thiserror::Error;

#[cfg(feature = "devel_cpal_rec")]
use cpal::traits::HostTrait;

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

    pub fn write_ogg(&self, filename:&str) -> Result<(), WavError> {

        match &self.buffer {
            Data::Raw(vec_data) => {
                let audio_raw = AudioRaw::new_raw(vec_data.clone(), self.samples_per_second);
                let as_ogg = audio_raw.to_ogg_opus().unwrap();
                let mut file = std::fs::File::create(filename).unwrap();
                file.write_all(&as_ogg).unwrap();
            }
            Data::Encoded(_) => {panic!("Can't transform to ogg an encoded audio");}
        }

        

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

    pub fn to_ogg_opus(&self) -> Result<Vec<u8>, WavError> {
        const FRAME_TIME_MS: u32 = 20;
        const FRAME_SAMPLES: u32 = 16000 * 1 * FRAME_TIME_MS / 1000;

        let mut buffer: Vec<u8> = Vec::new();
        {
            let mut packet_writer = ogg::PacketWriter::new(&mut buffer);
            let mut opus_encoder = opus::Encoder::new(16000, opus::Channels::Mono, opus::Application::Audio).unwrap();

            let max = {
                ((self.buffer.len() as f32 / FRAME_SAMPLES as f32).ceil() as u32) - 1
            };
            log::info!("Max {:?}", max);

            fn calc(counter: u32) -> usize {
                (counter as usize) * (FRAME_SAMPLES as usize)
            }
            const OPUS_HEAD: [u8; 19] = [
                b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd', // Magic header
                1, // Version number, always 1
                1, // Channels
                56, 1,//Pre-skip
                0, 0, 0, 0, // Original Hz (informational)
                0, 0, // Output gain
                0, // Channel map
                // If Channel map != 0, here should go channel mapping table
            ];

            let mut opus_tags : Vec<u8> = Vec::with_capacity(60);
            let vendor_str = format!("{}, lily {}", opus::version(), LILY_VER);
            opus_tags.extend(b"OpusTags");
            opus_tags.extend(&[vendor_str.len() as u8,0,0,0]);
            opus_tags.extend(vendor_str.bytes());
            opus_tags.extend(&[1,0,0,0]);
            opus_tags.extend(&[0;12]);

            packet_writer.write_packet(Box::new(OPUS_HEAD), 1, ogg::PacketWriteEndInfo::EndPage, 0).unwrap();
            packet_writer.write_packet(opus_tags.into_boxed_slice(), 1, ogg::PacketWriteEndInfo::EndPage, 0).unwrap();

            for counter in 0..max - 1{
                let pos_a: usize = calc(counter);
                let pos_b: usize = calc(counter + 1);

                
                let mut temp_buffer = [0; 256];
                let size = opus_encoder.encode(&self.buffer[pos_a..pos_b], temp_buffer.as_mut()).unwrap();
                let new_buffer = temp_buffer[0..size].to_owned();
                

                let end_info = {
                    if counter != max - 2 {
                        ogg::PacketWriteEndInfo::NormalPacket
                    }
                    else {
                        ogg::PacketWriteEndInfo::EndStream
                    }
                };

                packet_writer.write_packet(new_buffer.into_boxed_slice(), 1, end_info, (calc(counter + 1) as u64)*3).unwrap();
            }
        }

        Ok(buffer)
    }

}

#[derive(Error, Debug)]
pub enum WavError {
    #[error("this buffer size is too big ")]
    TooBig
}