use std::io::Cursor;
use std::process;

use crate::audio::AudioError;
use crate::vars::{DEFAULT_SAMPLES_PER_SECOND, LILY_VER, MAX_SAMPLES_PER_SECOND};

use byteorder::{LittleEndian, ByteOrder};
use ogg::{Packet, PacketReader, PacketWriter};
use magnum_opus::{Decoder as OpusDec, Encoder as OpusEnc};
use rand::Rng;

const fn to_samples(ms: u32, channels: u8, sps: u32) -> usize {
    ((sps * ms * channels as u32 ) / 1000) as usize
}

fn calc_samples(ms: f32, channels:u8, sps:u32) -> usize {
    let samps_ms = (sps as f32 * ms) as u32;
    ((samps_ms * channels as u32 ) / 1000) as usize
}

pub const OPUS_MAGIC_HEADER:[u8;8] = [b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd'];

pub struct PlayData {
    pub channels: u16
}

/**Reads audio from Ogg Opus, note: it only can read from the ones produced 
by itself, this is not ready for anything more*/
pub fn decode_ogg_opus(data: Vec<u8>, target_sps: u32) -> Result<(Vec<i16>, PlayData), AudioError> {
    
    const MAX_FRAME_TIME_MS: u32 = 60;
    const MAX_FRAME_SAMPLES: usize = to_samples(MAX_FRAME_TIME_MS, 2, MAX_SAMPLES_PER_SECOND);
    let mut reader = PacketReader::new(Cursor::new(&data));
    let fp = reader.read_packet_expected().map_err(|_| AudioError::MalformedAudio)?; // Header

    struct DecodeData {
        pre_skip: u16
    }

    // Analyze first page, where all the metadata we ened is contained
    fn check_fp(fp: &Packet) -> Result<(PlayData, DecodeData), AudioError> {
        // Read magic header
        if fp.data[0..8] != OPUS_MAGIC_HEADER {
            return Err(AudioError::MalformedAudio)
        }

        // Read version
        if fp.data[8] != 1 {
            return Err(AudioError::MalformedAudio)
        }
        
        Ok((
            PlayData{
                channels: fp.data[9] as u16, // Number of channels
            },
            DecodeData {
                pre_skip: LittleEndian::read_u16(&fp.data[10..12])
            }
        ))
    }

    let (play_data, dec_data) = check_fp(&fp)?;

    let chans = match play_data.channels {
        1 => Ok(magnum_opus::Channels::Mono),
        2 => Ok(magnum_opus::Channels::Stereo),
        _ => Err(AudioError::MalformedAudio)
    }?;

    // According to RFC7845 if a device supports 48Khz, decode at this rate
    let mut decoder =  OpusDec::new(target_sps, chans)?;

    // Vendor and other tags, we don't need them
    reader.read_packet_expected().map_err(|_| AudioError::MalformedAudio)?; // Tags
        
    let mut buffer: Vec<i16> = Vec::new();
    let mut frames_skip = dec_data.pre_skip as usize;
    while let Some(data) = reader.read_packet()? {
        let mut temp_buffer = [0; MAX_FRAME_SAMPLES];
        let out_size = decoder.decode(&data.data, &mut temp_buffer, false)?;
        
        if frames_skip < out_size {
            buffer.extend_from_slice(&temp_buffer[frames_skip..out_size]);
            frames_skip = 0;
        }
        else {
            frames_skip -= out_size;
        }

    }

    Ok( (buffer, play_data))
}
pub fn encode_ogg_opus(audio: &Vec<i16>) -> Result<Vec<u8>, AudioError> {

    // This should have a bitrate of 25.6 Kb/s, above the 24 Kb/s that IBM recomends

    // More frame time, sligtly less overhead more problematic packet loses,
    // a frame time of 20ms is considered good enough for most applications
    const FRAME_TIME_MS: u32 = 20;
    const NUM_CHANNELS: u8 = 1;
    const FRAME_SAMPLES: usize = to_samples(FRAME_TIME_MS, NUM_CHANNELS, DEFAULT_SAMPLES_PER_SECOND);
    const OPUS_CHANNELS: magnum_opus::Channels = magnum_opus::Channels::Mono;
    const S_PS: u32 = DEFAULT_SAMPLES_PER_SECOND;

    // Generate the serial which is nothing but a value to identify a stream, we
    // will also use the process id so that two lily implementations don't use 
    // the same serial even if getting one at the same time
    let mut rnd = rand::thread_rng();
    let serial = rnd.gen::<u32>() ^ process::id();
    let mut buffer: Vec<u8> = Vec::new();
    {
        let mut packet_writer = PacketWriter::new(&mut buffer);
        let mut opus_encoder = OpusEnc::new(S_PS, OPUS_CHANNELS, magnum_opus::Application::Audio)?;


        let max = match audio.len() {
            0 => 0,
            _ => ((audio.len() as f32 / FRAME_SAMPLES as f32).ceil() as u32) -1
        };

        fn calc(counter: u32) -> usize {
            (counter as usize) * FRAME_SAMPLES
        }

        const OPUS_HEAD: [u8; 19] = [
            b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd', // Magic header
            1, // Version number, always 1
            1, // Channels
            0, 0,//Pre-skip
            0, 0, 0, 0, // Original Hz (informational)
            0, 0, // Output gain
            0, // Channel map
            // If Channel map != 0, here should go channel mapping table
        ];
        let mut head = OPUS_HEAD;
        LittleEndian::write_u16(&mut head[10..12], 0); // Write pre-skip
        LittleEndian::write_u32(&mut head[12..16], S_PS); // Write Samples per second

        let mut opus_tags : Vec<u8> = Vec::with_capacity(60);
        let vendor_str = format!("{}, lily {}", magnum_opus::version(), LILY_VER);
        opus_tags.extend(b"OpusTags");
        opus_tags.extend(&[vendor_str.len() as u8,0,0,0]);
        opus_tags.extend(vendor_str.bytes());

        packet_writer.write_packet(Box::new(head), serial, ogg::PacketWriteEndInfo::EndPage, 0)?;
        packet_writer.write_packet(opus_tags.into_boxed_slice(), serial, ogg::PacketWriteEndInfo::EndPage, 0)?;

        // Do all frames
        for counter in 0..max{
            let pos_a: usize = calc(counter);
            let pos_b: usize = std::cmp::min(calc(counter + 1),audio.len());

            
            let mut temp_buffer = [0; FRAME_SAMPLES as usize];
            let size = opus_encoder.encode(&audio[pos_a..pos_b], temp_buffer.as_mut())?;
            let new_buffer = temp_buffer[0..size].to_owned().into_boxed_slice();
            

            let end_info = {
                if pos_b == audio.len() {
                    ogg::PacketWriteEndInfo::EndStream
                }
                else {
                    ogg::PacketWriteEndInfo::NormalPacket
                }
            };

            packet_writer.write_packet(new_buffer, serial, end_info, (calc(counter + 1) as u64)*3)?;
        }

        // Calc the biggest frame buffer that still is either smaller or the
        // same size as the input
        fn calc_biggest_spills<T:PartialOrd + Copy>(val: T, possibles: &[T]) -> Option<T> {
            for container in possibles.iter().rev()  {
                if *container <= val {
                    return Some(*container)
                }
            }
            None
        }

        // Try to add as less of empty audio as possible, first everything into
        // small frames, and on the last one, if needed fill with 0, since the
        // last one is going to be smaller this should be much less of a problem
        let mut last_sample = calc(max);
        if last_sample < audio.len() {
            let frames_sizes = [
                    calc_samples(2.5, NUM_CHANNELS, S_PS),
                    calc_samples(5.0, NUM_CHANNELS, S_PS),
                    calc_samples(10.0, NUM_CHANNELS, S_PS),
                    calc_samples(20.0, NUM_CHANNELS, S_PS)
            ];

            while last_sample < audio.len() {
                let rem_samples = audio.len() - last_sample;

                fn write_audio(
                    opus_encoder:&mut OpusEnc,
                    in_buffer: &[i16],
                    packet_writer: &mut PacketWriter<&mut Vec<u8>>,
                    serial: u32,
                    abgsp: u64
                ) -> Result<(), AudioError> {
                    let mut temp_buffer = [0; FRAME_SAMPLES as usize];
                    let size = opus_encoder.encode(&in_buffer[..], temp_buffer.as_mut())?;
                    let new_buffer = temp_buffer[0..size].to_owned().into_boxed_slice();
                    packet_writer.write_packet(
                        new_buffer,
                        serial, 
                        ogg::PacketWriteEndInfo::EndStream,
                        abgsp
                    )?;
                    Ok(())
                }

                match calc_biggest_spills(rem_samples, &frames_sizes) {
                    Some(frame_size) => {
                        let mut in_buffer = Vec::with_capacity(frame_size);
                        in_buffer.resize(frame_size, 0);
                        in_buffer.copy_from_slice(&audio[last_sample .. last_sample + frame_size]);
                        last_sample += frame_size;

                        write_audio(&mut opus_encoder, &in_buffer[..], &mut packet_writer, serial, (last_sample as u64) *3)?;
                    }
                    None => {
                        let frame_size = frames_sizes[0];

                        // Prepare our new buffer for the whole frame, but it's
                        // size is still 0
                        let mut in_buffer = Vec::with_capacity(frame_size);
                        in_buffer.resize(rem_samples, 0); // Add audio samples
                        in_buffer.copy_from_slice(&audio[last_sample..]);
                        in_buffer.resize(frame_size, 0);

                        last_sample = audio.len(); // We end this here

                        write_audio(
                            &mut opus_encoder,
                            &in_buffer[..],
                            &mut packet_writer,
                            serial,
                            (calc(max + 1) as u64)*3
                        )?;
                    }
                }
                
            }
        }

    }

    Ok(buffer)
}