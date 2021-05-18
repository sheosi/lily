pub mod audio;
pub mod communication;
#[cfg(feature = "client")]
pub mod client;
pub mod other;
pub mod vars;


#[cfg(test)]
mod tests {
    
    mod opus {
        use std::io::Cursor;
        use crate::audio::{AudioRaw, encode_ogg_opus, decode_ogg_opus,RecDevice};
        use crate::vars::DEFAULT_SAMPLES_PER_SECOND;
        use anyhow::Result;
        use serial_test::serial;

        #[test]
        fn dec_enc_empty() -> Result<()> {
            let audio = AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND);
            let (opus,enc_fin_range) = encode_ogg_opus(&audio.buffer)?;
            let (audio2, _,dec_fin_range) = decode_ogg_opus::<_,16000>(Cursor::new(opus))?;
            assert_eq!(audio.buffer.len(), audio2.len()); // Should be the same, empty
            assert_eq!(enc_fin_range, dec_fin_range);

            Ok(())
        }

        #[test]
        #[serial]
        fn dec_enc_recording_big() -> Result<()> {
            let mut rec_dev = RecDevice::new();
            rec_dev.start_recording()?;
            std::thread::sleep(std::time::Duration::from_millis(100));
            let audio = AudioRaw::new_raw(rec_dev.read()?.expect("No audio").to_owned(), DEFAULT_SAMPLES_PER_SECOND);
            rec_dev.stop_recording()?;
            let (opus,enc_fin_range) = encode_ogg_opus(&audio.buffer)?;
            let (a2,_,dec_fin_range) = decode_ogg_opus::<_,16000>(Cursor::new(opus))?;
            assert_eq!(dec_fin_range, enc_fin_range);
            assert_eq!(audio.len(), a2.len());
            Ok(())
        }

        #[test]
        #[serial]
        fn dec_enc_recording_small() -> Result<()> {
            let mut rec_dev = RecDevice::new();
            rec_dev.start_recording()?;
            std::thread::sleep(std::time::Duration::from_millis(20));
            let audio = AudioRaw::new_raw(rec_dev.read()?.expect("No audio").to_owned(), DEFAULT_SAMPLES_PER_SECOND);
            rec_dev.stop_recording()?;
            let (opus, enc_fin_range) = encode_ogg_opus(&audio.buffer)?;
            let (a2, _, dec_fin_range) = decode_ogg_opus::<_, 16000>(Cursor::new(opus))?;
            assert_eq!(dec_fin_range, enc_fin_range);
            assert_eq!(audio.len(), a2.len());
            Ok(())
        }

        #[test]
        #[serial]
        // Record, encode, decode , encode and decode again, finally compare the
        // first and second decodes, to make sure nothing is lost (can't compare
        // raw audio as vorbis is lossy)
        fn dec_enc_recording_whole() -> Result<()> {
            let mut rec_dev = RecDevice::new();
            rec_dev.start_recording()?;
            std::thread::sleep(std::time::Duration::from_millis(40));
            let audio = AudioRaw::new_raw(rec_dev.read()?.expect("No audio").to_owned(), DEFAULT_SAMPLES_PER_SECOND);
            rec_dev.stop_recording()?;
            let (opus, enc_fr1) = encode_ogg_opus(&audio.buffer)?;
            let (audio2, _, dec_fr1) = decode_ogg_opus::<_, 16000>(Cursor::new(opus))?;
            let (opus2, enc_fr2) = encode_ogg_opus(&audio2)?;
            let (audio3, _, dec_fr2) = decode_ogg_opus::<_, 16000>(Cursor::new(opus2))?;
            assert_eq!(audio2.len(), audio3.len());
            assert_eq!(enc_fr1, dec_fr1);
            assert_eq!(enc_fr2, dec_fr2);
            Ok(())
        }
    }
}
