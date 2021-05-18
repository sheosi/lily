use std::cmp::min;
use std::io::{Read, Seek};
use std::process;



use crate::audio::AudioError;
use crate::vars::{DEFAULT_SAMPLES_PER_SECOND, LILY_VER};
use byteorder::{LittleEndian, ByteOrder};
use ogg::{Packet, PacketReader, PacketWriter};
use magnum_opus::{Bitrate, Decoder as OpusDec, Encoder as OpusEnc};
use rand::Rng;


const fn to_samples<const TARGET_SPS: u32>(ms: u32) -> usize {
    ((TARGET_SPS * ms) / 1000) as usize
}


// In microseconds
const fn calc_fr_size(us: u32, channels:u8, sps:u32) -> usize {
    let samps_ms = (sps * us) as u32;
    const US_TO_MS: u32 = 10;
    ((samps_ms * channels as u32 ) / (1000 * US_TO_MS )) as usize
}

const fn calc_sr(val:u16, org_sr: u32, dest_sr: u32) -> u16 {
    ((val as u32 * dest_sr) /org_sr) as u16
}
const fn calc_sr_u64(val:u64, org_sr: u32, dest_sr: u32) -> u64 {
    (val * dest_sr as u64) /(org_sr as u64)
}

const fn opus_channels(val: u8) -> magnum_opus::Channels{
    if val == 0 {
        // Never should be 0
        magnum_opus::Channels::Mono
    }
    else if val == 1 {
       magnum_opus::Channels::Mono
    }
    else {
       magnum_opus::Channels::Stereo
    }
}
// We use this to check whether a file is ogg opus or not inside the client
pub const OPUS_MAGIC_HEADER:[u8;8] = [b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd'];
const OGG_OPUS_SPS: u32 = 48000;

pub struct PlayData {
    pub channels: u16
}

/**Reads audio from Ogg Opus, note: it only can read from the ones produced 
by itself, this is not ready for anything more, third return is final range just
available while testing, otherwise it is a 0*/
pub fn decode_ogg_opus<T: Read + Seek, const TARGET_SPS: u32>(data: T) -> Result<(Vec<i16>, PlayData, u32), AudioError> {
    // Data
    const MAX_NUM_CHANNELS: u8 = 2;
    const MAX_FRAME_SAMPLES: usize = 5760; // According to opus_decode docs
    const MAX_FRAME_SIZE: usize = MAX_FRAME_SAMPLES * (MAX_NUM_CHANNELS as usize); // Our buffer will be i16 so, don't convert to bytes

    let mut reader = PacketReader::new(data);
    let fp = reader.read_packet_expected().map_err(|_| AudioError::MalformedAudio)?; // Header

    struct DecodeData {
        pre_skip: u16,
        gain: i32
    }

    // Analyze first page, where all the metadata we need is contained
    fn check_fp<const TARGET_SPS: u32>(fp: &Packet) -> Result<(PlayData, DecodeData), AudioError> {

        // Check size
        if fp.data.len() < 19 {
            return Err(AudioError::MalformedAudio)
        }

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
                pre_skip: calc_sr(LittleEndian::read_u16(&fp.data[10..12]), OGG_OPUS_SPS, TARGET_SPS),
                gain: LittleEndian::read_i16(&fp.data[16..18]) as i32
            }
        ))
    }

    let (play_data, dec_data) = check_fp::<TARGET_SPS>(&fp)?;

    let chans = match play_data.channels {
        1 => Ok(magnum_opus::Channels::Mono),
        2 => Ok(magnum_opus::Channels::Stereo),
        _ => Err(AudioError::MalformedAudio)
    }?;

    // According to RFC7845 if a device supports 48Khz, decode at this rate
    let mut decoder =  OpusDec::new(TARGET_SPS, chans)?;
    decoder.set_gain(dec_data.gain)?;

    // Vendor and other tags, do a basic check
    let sp = reader.read_packet_expected().map_err(|_| AudioError::MalformedAudio)?; // Tags
    fn check_sp(sp: &Packet) -> Result<(), AudioError> {
        if sp.data.len() < 12 {
            return Err(AudioError::MalformedAudio)
        }

        let head = std::str::from_utf8(&sp.data[0..8]).or_else(|_| Err(AudioError::MalformedAudio))?;
        if head != "OpusTags" {
            return Err(AudioError::MalformedAudio)
        }
        
        Ok(())
    }
        
    check_sp(&sp)?;

    let mut buffer: Vec<i16> = Vec::new();
    let mut rem_skip = dec_data.pre_skip as usize;
    let mut dec_absgsp = 0;
    while let Some(packet) = reader.read_packet()? {
        let mut temp_buffer = [0; MAX_FRAME_SIZE];
        let out_size = decoder.decode(&packet.data, &mut temp_buffer, false)?;
        let absgsp = calc_sr_u64(packet.absgp_page(),OGG_OPUS_SPS, TARGET_SPS) as usize;
        dec_absgsp += out_size;
        let trimmed_end = if packet.last_in_stream() && dec_absgsp > absgsp {
            (out_size as usize * play_data.channels as usize) - (dec_absgsp - absgsp)
        }
        else {
            // out_size == num of samples *per channel*
            out_size as usize * play_data.channels as usize
        } as usize;

        if rem_skip < out_size {
            buffer.extend_from_slice(&temp_buffer[rem_skip..trimmed_end]);
            rem_skip = 0;
        }
        else {
            rem_skip -= out_size;
        }

    }

    let final_range= if cfg!(test) {decoder.get_final_range()?}
                         else{0};

    Ok( (buffer, play_data, final_range))
}

pub fn encode_ogg_opus(audio: &Vec<i16>) -> Result<(Vec<u8>, u32), AudioError> {

    // This should have a bitrate of 24 Kb/s, exactly what IBM recommends

    // More frame time, sligtly less overhead more problematic packet loses,
    // a frame time of 20ms is considered good enough for most applications


    // Config
    const S_PS :u32 = DEFAULT_SAMPLES_PER_SECOND;
    const NUM_CHANNELS: u8 = 1;
    
    // Data
    const FRAME_TIME_MS: u32 = 20;
    const FRAME_SAMPLES: usize = to_samples::<S_PS>(FRAME_TIME_MS);
    const FRAME_SIZE: usize = FRAME_SAMPLES * (NUM_CHANNELS as usize);
    const MAX_PACKET: usize = 4000; // Maximum theorical recommended by Opus

    // Generate the serial which is nothing but a value to identify a stream, we
    // will also use the process id so that two lily implementations don't use 
    // the same serial even if getting one at the same time
    let mut rnd = rand::thread_rng();
    let serial = rnd.gen::<u32>() ^ process::id();
    let mut buffer: Vec<u8> = Vec::new();
    
    let mut packet_writer = PacketWriter::new(&mut buffer);
    let mut opus_encoder = OpusEnc::new(S_PS, opus_channels(NUM_CHANNELS), magnum_opus::Application::Audio)?;
    opus_encoder.set_bitrate(Bitrate::Bits(24000))?;
    let skip = opus_encoder.get_lookahead().unwrap() as u16;
    let skip_us = skip as usize;
    let tot_samples = audio.len() + skip_us;
    let skip_48 = calc_sr(
        skip,
        DEFAULT_SAMPLES_PER_SECOND,
        OGG_OPUS_SPS
    );

    let max = (tot_samples as f32 / FRAME_SIZE as f32).floor() as u32;

    const fn calc(counter: u32) -> usize {
        (counter as usize) * FRAME_SIZE
    }

    const fn calc_samples(counter:u32) -> usize {
        (counter as usize) * FRAME_SAMPLES
    }

    const fn granule<const S_PS: u32>(val: usize) -> u64 {
        calc_sr_u64(val as u64, S_PS, OGG_OPUS_SPS)
    }

    const OPUS_HEAD: [u8; 19] = [
        b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd', // Magic header
        1, // Version number, always 1
        NUM_CHANNELS, // Channels
        0, 0,//Pre-skip
        0, 0, 0, 0, // Original Hz (informational)
        0, 0, // Output gain
        0, // Channel map family
        // If Channel map != 0, here should go channel mapping table
    ];

    fn encode_with_skip(opus_encoder: &mut OpusEnc, audio: &[i16], pos_a: usize, pos_b: usize, skip_us: usize) -> Result<Box<[u8]>, AudioError> {
        let res = if pos_a > skip_us {
            opus_encoder.encode_vec(&audio[pos_a-skip_us..pos_b-skip_us], MAX_PACKET)
        }
        else {
            let mut buf = Vec::with_capacity(pos_b-pos_a);
            buf.resize(pos_b-pos_a, 0);
            if pos_b > skip_us {
                buf[skip_us - pos_a..].copy_from_slice(&audio[.. pos_b - skip_us]);
            }
            opus_encoder.encode_vec(&buf, MAX_PACKET)
        };
        Ok(res?.into_boxed_slice())
    }

    fn is_end_of_stream(pos: usize, max: usize) -> ogg::PacketWriteEndInfo {
        if pos == max {
            ogg::PacketWriteEndInfo::EndStream
        }
        else {
            ogg::PacketWriteEndInfo::NormalPacket
        }
    }

    let mut head = OPUS_HEAD;
    LittleEndian::write_u16(&mut head[10..12], skip_48 as u16); // Write pre-skip
    LittleEndian::write_u32(&mut head[12..16], S_PS); // Write Samples per second

    let mut opus_tags : Vec<u8> = Vec::with_capacity(60);
    let vendor_str = format!("{}, lily {}", magnum_opus::version(), LILY_VER);
    opus_tags.extend(b"OpusTags");
    let mut len_bf = [0u8;4];
    LittleEndian::write_u32(&mut len_bf, vendor_str.len() as u32);
    opus_tags.extend(&len_bf);
    opus_tags.extend(vendor_str.bytes());
    opus_tags.extend(&[0]); // No user comments

    packet_writer.write_packet(Box::new(head), serial, ogg::PacketWriteEndInfo::EndPage, 0)?;
    packet_writer.write_packet(opus_tags.into_boxed_slice(), serial, ogg::PacketWriteEndInfo::EndPage, 0)?;

    // Do all frames
    for counter in 0..max{ // Last value of counter is max - 1 
        let pos_a: usize = calc(counter);
        let pos_b: usize = calc(counter + 1);
        
        assert!((pos_b - pos_a) <= FRAME_SIZE);
        
        let new_buffer = encode_with_skip(&mut opus_encoder, audio, pos_a, pos_b, skip_us)?;

        packet_writer.write_packet(
            new_buffer,
            serial,
            is_end_of_stream(pos_b, tot_samples),
            granule::<S_PS>(skip_us + calc_samples(counter + 1)
        ))?;
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

    fn encode_no_skip(opus_encoder: &mut OpusEnc, audio: &[i16], start: usize, frame_size : usize) -> Result<Box<[u8]>, AudioError> {
        let temp_buffer = opus_encoder.encode_vec(&audio[start .. start + frame_size], MAX_PACKET)?;
        Ok(temp_buffer.to_owned().into_boxed_slice())
    }

    // Try to add as less of empty audio as possible, first everything into
    // small frames, and on the last one, if needed fill with 0, since the
    // last one is going to be smaller this should be much less of a problem
    let mut last_sample = calc(max);
    assert!(last_sample <= audio.len() + skip_us);
    const FRAMES_SIZES: [usize; 4] = [
            calc_fr_size(25, NUM_CHANNELS, S_PS),
            calc_fr_size(50, NUM_CHANNELS, S_PS),
            calc_fr_size(100, NUM_CHANNELS, S_PS),
            calc_fr_size(200, NUM_CHANNELS, S_PS)
    ];

    while last_sample < tot_samples {

            let rem_samples = tot_samples - last_sample;
            let last_audio_s = last_sample - min(last_sample,skip_us);

            match calc_biggest_spills(rem_samples, &FRAMES_SIZES) {
                Some(frame_size) => {
                    let enc = if last_sample >= skip_us {
                        encode_no_skip(&mut opus_encoder, audio, last_audio_s, frame_size)?
                    }
                    else {
                        encode_with_skip(&mut opus_encoder, audio, last_sample, last_sample + frame_size, skip_us)?
                    };
                    last_sample += frame_size;
                    packet_writer.write_packet(
                        enc,
                        serial, 
                        is_end_of_stream(last_sample, tot_samples),
                        granule::<S_PS>(last_sample/(NUM_CHANNELS as usize))
                    )?;
                }
                None => {
                    let mut in_buffer = [0i16;FRAMES_SIZES[0]];
                    let rem_skip = skip_us - min(last_sample, skip_us);
                    in_buffer[rem_skip..rem_samples].copy_from_slice(&audio[last_audio_s..]);

                    last_sample = tot_samples; // We end this here
                    
                    packet_writer.write_packet(
                        encode_no_skip(&mut opus_encoder, &in_buffer, 0, FRAMES_SIZES[0])?,
                        serial, 
                        ogg::PacketWriteEndInfo::EndStream,
                        granule::<S_PS>((skip_us + audio.len())/(NUM_CHANNELS as usize))
                    )?;
                    
                }
            }
            
        }

    let final_range = if cfg!(test) {opus_encoder.get_final_range()?}
                          else {0};

    Ok((buffer, final_range))
}