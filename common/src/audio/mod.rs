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
use crate::vars::{DEFAULT_SAMPLES_PER_SECOND, LILY_VER};

use byteorder::{LittleEndian, ByteOrder};
use ogg::{Packet, PacketReader, PacketWriter};
use opus::{Decoder as OpusDec, Encoder as OpusEnc};
use log::warn;
use thiserror::Error;

const OPUS_MAGIC_HEADER:[u8;8] = [b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd'];
struct AudioEncoded {
    data: Vec<u8>
}

/**Reads audio from Ogg Opus, note: it only can read from the ones produced 
    by itself, this is not ready for anything more*/
pub fn decode_ogg_opus(data: Vec<u8>) -> Result<(Vec<i16>, PlayData), AudioError> {
    let mut reader = PacketReader::new(Cursor::new(&data));
    let mut decoder =  OpusDec::new(DEFAULT_SAMPLES_PER_SECOND, opus::Channels::Mono)?;
    let fp = reader.read_packet_expected().map_err(|_| AudioError::MalformedAudio)?; // Header

    fn check_fp(fp: &Packet) -> Result<PlayData, AudioError> {
        // Read magic header
        if fp.data[0..8] != OPUS_MAGIC_HEADER {
            return Err(AudioError::MalformedAudio)
        }

        // Read version
        if fp.data[8] != 1 {
            return Err(AudioError::MalformedAudio)
        }

        // Pre-skip
        //fp.data[10]
        //fp.data[11]
        let sps = LittleEndian::read_u32(&fp.data[12..16]);
        
        Ok(PlayData{
            channels: fp.data[9] as u16, // Number of channels
            sps
        })
    }

    let play_data = check_fp(&fp)?;

    reader.read_packet_expected().map_err(|_| AudioError::MalformedAudio)?; // Tags
        
    let mut buffer: Vec<i16> = Vec::new();
    while let Some(data) = reader.read_packet()? {
        let mut temp_buffer = [0; FRAME_SAMPLES as usize];
        let out_size = decoder.decode(&data.data, &mut temp_buffer, false)?;
        buffer.extend_from_slice(&temp_buffer[0..out_size]);
    }

    Ok( (buffer, play_data))
}


impl AudioEncoded {
    fn new(data: Vec<u8>) -> Self {
        Self {data}
    }

    pub fn is_ogg_opus(&self) -> bool {
        self.data.len() > 36 && self.data[28..36] == OPUS_MAGIC_HEADER
    }

    pub fn get_sps(&self) -> u32 {
        // Just some value, not yet implemented
        // TODO: Finish it
        warn!("AudioEncoded::get_sps not yet implemented");
        48000
    }
}

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
            Data::Raw(data_self) => data_self.append_audio(b, DEFAULT_SAMPLES_PER_SECOND).unwrap(),
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

// This should have a bitrate of 25.6 Kb/s, above the 24 Kb/s that IBM recomends

// More frame time, sligtly less overhead more problematic packet loses,
// a frame time of 20ms is considered good enough for most applications
const FRAME_TIME_MS: u32 = 20;
const FRAME_SAMPLES: u32 = (16000 * 1 * FRAME_TIME_MS) / 1000;

pub struct PlayData {
    channels: u16,
    sps: u32
}

// For managing raw audio, mostly coming from the mic,
// is fixed at 16 KHz and mono (what most STTs )
#[derive(Debug, Eq, PartialEq)]
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

    pub fn to_ogg_opus(&self) -> Result<Vec<u8>, AudioError> {
        let SPS = DEFAULT_SAMPLES_PER_SECOND;
        let mut buffer: Vec<u8> = Vec::new();
        {
            let mut packet_writer = PacketWriter::new(&mut buffer);
            let mut opus_encoder = OpusEnc::new(SPS, opus::Channels::Mono, opus::Application::Audio)?;


            let max = {
                match self.buffer.len() {
                    0 => 0,
                    _ => ((self.buffer.len() as f32 / FRAME_SAMPLES as f32).ceil() as u32) - 1
                }
            };

            fn calc(counter: u32) -> usize {
                (counter as usize) * (FRAME_SAMPLES as usize)
            }

            const OPUS_HEAD: [u8; 19] = [
                b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd', // Magic header
                1, // Version number, always 1
                1, // Channels
                56, 1,//Pre-skip
                0, 0, 0, 0, // Original Hz (informational), the numbers here should mean 16000
                0, 0, // Output gain
                0, // Channel map
                // If Channel map != 0, here should go channel mapping table
            ];
            let mut head = OPUS_HEAD;
            LittleEndian::write_u32(&mut head[12..16], SPS);

            let mut opus_tags : Vec<u8> = Vec::with_capacity(60);
            let vendor_str = format!("{}, lily {}", opus::version(), LILY_VER);
            opus_tags.extend(b"OpusTags");
            opus_tags.extend(&[vendor_str.len() as u8,0,0,0]);
            opus_tags.extend(vendor_str.bytes());
            //opus_tags.extend(&[1,0,0,0]); // Not sure what is this
            //opus_tags.extend(&[0;12]); // Not sure what is this

            packet_writer.write_packet(Box::new(head), 1, ogg::PacketWriteEndInfo::EndPage, 0)?;
            packet_writer.write_packet(opus_tags.into_boxed_slice(), 1, ogg::PacketWriteEndInfo::EndPage, 0)?;

            for counter in 0..max{
                let pos_a: usize = calc(counter);
                let pos_b: usize = calc(counter + 1);

                
                let mut temp_buffer = [0; FRAME_SAMPLES as usize];
                let size = opus_encoder.encode(&self.buffer[pos_a..std::cmp::min(pos_b, self.buffer.len())], temp_buffer.as_mut())?;
                let new_buffer = temp_buffer[0..size].to_owned();
                

                let end_info = {
                    if counter == max - 1 {
                        ogg::PacketWriteEndInfo::EndStream
                    }
                    else {
                        ogg::PacketWriteEndInfo::NormalPacket
                    }
                };

                packet_writer.write_packet(new_buffer.into_boxed_slice(), 1, end_info, (calc(counter + 1) as u64)*3)?;
            }
        }

        Ok(buffer)
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
}


#[derive(Error, Debug)]
pub enum AudioError {
    #[error("Io Error")]
    IOError(#[from] std::io::Error),

    #[error("Encoding error")]
    OpusError(#[from] opus::Error),

    #[error("Input audio was malformed")]
    MalformedAudio,

    #[error("Failed to decode ogg")]
    OggReadError(#[from] ogg::OggReadError),

    #[error("Incompatible Samples per seconds")]
    IncompatibleSps
}