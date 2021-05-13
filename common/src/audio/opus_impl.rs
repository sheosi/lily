use std::io::Cursor;
use std::process;



use crate::audio::AudioError;
use crate::vars::{DEFAULT_SAMPLES_PER_SECOND, LILY_VER};
#[cfg(feature="rust_opus")]
use byteorder::{LittleEndian, ByteOrder};
#[cfg(feature="rust_opus")]
use ogg::{Packet, PacketReader, PacketWriter};
#[cfg(feature="rust_opus")]
use magnum_opus::{Bitrate, Decoder as OpusDec, Encoder as OpusEnc};
#[cfg(feature="rust_opus")]
use rand::Rng;

#[cfg(not(feature="rust_opus"))]
use opusfile::OggOpusFile;

const fn to_samples<const TARGET_SPS: u32>(ms: u32) -> usize {
    ((TARGET_SPS * ms) / 1000) as usize
}


// In microseconds
const fn calc_fr_size(us: u32, channels:u8, sps:u32) -> usize {
    let samps_ms = (sps * us) as u32;
    ((samps_ms * channels as u32 ) / 1000) as usize
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
#[cfg(feature="rust_opus")]
pub fn decode_ogg_opus<const TARGET_SPS: u32>(data: Vec<u8>) -> Result<(Vec<i16>, PlayData, u32), AudioError> {
    // Data
    const MAX_NUM_CHANNELS: u8 = 2;
    const MAX_FRAME_SAMPLES: usize = 5760; // According to opus_decode docs
    const MAX_FRAME_SIZE: usize = MAX_FRAME_SAMPLES * (MAX_NUM_CHANNELS as usize); // Our buffer will be i16 so, don't convert to bytes

    let mut reader = PacketReader::new(Cursor::new(&data));
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
    let mut pre_skip = dec_data.pre_skip as usize;
    let mut last_absgp = 0;
    while let Some(packet) = reader.read_packet()? {
        let mut temp_buffer = [0; MAX_FRAME_SIZE];
        let out_size = decoder.decode(&packet.data, &mut temp_buffer, false)?;
        let absgsp = packet.absgp_page();
        if absgsp < last_absgp {
            return Err(AudioError::MalformedAudio);
        }

        let trimmed_end = if packet.last_in_stream() && absgsp - last_absgp < out_size as u64 {
            calc_sr_u64((absgsp - last_absgp)/(play_data.channels as u64) , OGG_OPUS_SPS, TARGET_SPS)
        }
        else {
            // out_size == num of samples *per channel*
            out_size as u64 * play_data.channels as u64
        } as usize;
        last_absgp = absgsp;


        if pre_skip < out_size {
            buffer.extend_from_slice(&temp_buffer[pre_skip..trimmed_end]);
            pre_skip = 0;
        }
        else {
            pre_skip -= out_size;
        }

    }

    let final_range= if cfg!(test) {decoder.get_final_range()?}
                         else{0};

    Ok( (buffer, play_data, final_range))
}
#[cfg(not(feature="rust_opus"))]
pub fn decode_ogg_opus(data: Vec<u8>, target_sps: u32) -> Result<(Vec<i16>, PlayData, u32), AudioError> {
    let a = OggOpusFile::from_read(Cursor::new(&data)).unwrap();
    a.read();
    
}
#[cfg(feature="rust_opus")]
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
    const MAX_PACKET: usize = 1500; // Could've been anything else, really

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
    let skip_48 = calc_sr(
        skip,
        DEFAULT_SAMPLES_PER_SECOND,
        OGG_OPUS_SPS
    );

    let max = match audio.len() {
        0 => 0,
        _ => ((audio.len() as f32 / FRAME_SIZE as f32).floor() as u32) // This -1 is to move the start of the range to 0
    };

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
    for counter in 0..max{
        let pos_a: usize = calc(counter);
        let pos_b: usize = calc(counter + 1);

        
        assert!((pos_b - pos_a) <= FRAME_SIZE);
        let new_buffer = opus_encoder.encode_vec(&audio[pos_a..pos_b], MAX_PACKET)?.into_boxed_slice();
            

        let end_info = {
            if pos_b == audio.len() {
                ogg::PacketWriteEndInfo::EndStream
            }
            else {
                ogg::PacketWriteEndInfo::NormalPacket
            }
        };

        packet_writer.write_packet(new_buffer, serial, end_info, granule::<S_PS>(skip as usize + calc_samples(counter + 1)))?;
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
        const FRAMES_SIZES: [usize; 4] = [
                calc_fr_size(25, NUM_CHANNELS, S_PS),
                calc_fr_size(50, NUM_CHANNELS, S_PS),
                calc_fr_size(100, NUM_CHANNELS, S_PS),
                calc_fr_size(200, NUM_CHANNELS, S_PS)
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

            match calc_biggest_spills(rem_samples, &FRAMES_SIZES) {
                Some(frame_size) => {
                    let mut in_buffer = Vec::with_capacity(frame_size);
                    in_buffer.resize(frame_size, 0);
                    in_buffer.copy_from_slice(&audio[last_sample .. last_sample + frame_size]);
                    last_sample += frame_size;

                    write_audio(&mut opus_encoder, &in_buffer[..], &mut packet_writer, serial, granule::<S_PS>(skip as usize + last_sample/(NUM_CHANNELS as usize)))?;
                }
                None => {
                    const FRAME_SIZE: usize = FRAMES_SIZES[0];

                    // Prepare our new buffer for the whole frame, but it's
                    // size is still 0
                    let mut in_buffer = Vec::with_capacity(FRAME_SIZE);
                    in_buffer.resize(rem_samples, 0); // Add audio samples
                    in_buffer.copy_from_slice(&audio[last_sample..]);
                    in_buffer.resize(FRAME_SIZE, 0);

                    last_sample = audio.len(); // We end this here

                    write_audio(
                        &mut opus_encoder,
                        &in_buffer[..],
                        &mut packet_writer,
                        serial,
                        granule::<S_PS>(skip as usize + audio.len()/(NUM_CHANNELS as usize)),
                    )?;
                }
            }
            
        }
    
    }
    let final_range = if cfg!(test) {opus_encoder.get_final_range()?}
                          else {0};

    Ok((buffer, final_range))
}
#[cfg(not(feature="rust_opus"))]
pub fn encode_ogg_opus(audio: &Vec<i16>) -> Result<(Vec<u8>, u32), AudioError> {
}