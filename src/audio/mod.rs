mod playdevice;
mod recdevice;

pub use self::playdevice::*;
pub use self::recdevice::*;

use std::io::Write;
use crate::vars::LILY_VER;
use thiserror::Error;

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

    fn len(&self) -> usize {
        match self {
            Data::Raw(buffer) => buffer.len(),
            Data::Encoded(buffer) => buffer.len()
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

    pub fn write_ogg(&self, filename:&str) -> Result<(), AudioError> {

        match &self.buffer {
            Data::Raw(vec_data) => {
                let audio_raw = AudioRaw::new_raw(vec_data.clone(), self.samples_per_second);
                let as_ogg = audio_raw.to_ogg_opus()?;
                let mut file = std::fs::File::create(filename)?;
                file.write_all(&as_ogg)?;
            }
            Data::Encoded(_) => {panic!("Can't transform to ogg an encoded audio");}
        }

        

        Ok(())
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    // Length in seconds
    pub fn len_s(&self) -> f32 {
        let len = self.buffer.len();
        (len as f32)/(self.samples_per_second as f32)
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

    pub fn to_ogg_opus(&self) -> Result<Vec<u8>, AudioError> {
        const FRAME_TIME_MS: u32 = 20;
        const FRAME_SAMPLES: u32 = 16000 * 1 * FRAME_TIME_MS / 1000;

        let mut buffer: Vec<u8> = Vec::new();
        {
            let mut packet_writer = ogg::PacketWriter::new(&mut buffer);
            let mut opus_encoder = opus::Encoder::new(16000, opus::Channels::Mono, opus::Application::Audio)?;

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

            packet_writer.write_packet(Box::new(OPUS_HEAD), 1, ogg::PacketWriteEndInfo::EndPage, 0)?;
            packet_writer.write_packet(opus_tags.into_boxed_slice(), 1, ogg::PacketWriteEndInfo::EndPage, 0)?;

            for counter in 0..max - 1{
                let pos_a: usize = calc(counter);
                let pos_b: usize = calc(counter + 1);

                
                let mut temp_buffer = [0; 256];
                let size = opus_encoder.encode(&self.buffer[pos_a..pos_b], temp_buffer.as_mut())?;
                let new_buffer = temp_buffer[0..size].to_owned();
                

                let end_info = {
                    if counter != max - 2 {
                        ogg::PacketWriteEndInfo::NormalPacket
                    }
                    else {
                        ogg::PacketWriteEndInfo::EndStream
                    }
                };

                packet_writer.write_packet(new_buffer.into_boxed_slice(), 1, end_info, (calc(counter + 1) as u64)*3)?;
            }
        }

        Ok(buffer)
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
        (len as f32)/(self.samples_per_second as f32)
    }
}

#[derive(Error, Debug)]
pub enum AudioError {
    #[error("Io Error")]
    IOError(#[from] std::io::Error),
    #[error("Encoding error")]
    OpusError(#[from] opus::Error)
}