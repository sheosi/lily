pub mod audio;
pub mod communication;
#[cfg(feature = "client")]
pub mod hotword;
pub mod other;
#[cfg(feature = "client")]
pub mod vad;
pub mod vars;


#[cfg(test)]
mod tests {
    
    mod opus {
        use crate::audio::{AudioRaw, decode_ogg_opus,RecDevice};
        use crate::vars::DEFAULT_SAMPLES_PER_SECOND;
        use anyhow::Result;

        #[test]
        fn dec_enc_empty() -> Result<()> {
            let audio = AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND);
            let opus = audio.to_ogg_opus()?;
            let (audio2, _) = decode_ogg_opus(opus, DEFAULT_SAMPLES_PER_SECOND)?;
            assert_eq!(audio.buffer, audio2); // Should be the same, empty

            Ok(())
        }

        #[test]
        // Encode and decode in Ogg Opus a recording of 50ms, which does not fit
        // exactly into the 20ms frame size that is used
        fn dec_enc_recording_inexact() -> Result<()> {
            let mut rec_dev = RecDevice::new();
            rec_dev.start_recording()?;
            std::thread::sleep(std::time::Duration::from_millis(50));
            let audio = AudioRaw::new_raw(rec_dev.read()?.expect("No audio").to_owned(), DEFAULT_SAMPLES_PER_SECOND);
            rec_dev.stop_recording()?;
            let opus = audio.to_ogg_opus()?;
            let audio2 = decode_ogg_opus(opus, DEFAULT_SAMPLES_PER_SECOND)?;
            assert_eq!(audio.len(), audio2.0.len());
            Ok(())
        }

        #[test]
        // Encode and decode in Ogg Opus a recording of 40ms, which fits exactly
        // into the 20ms frame size that is used
        fn dec_enc_recording_exact() -> Result<()> {
            let mut rec_dev = RecDevice::new();
            rec_dev.start_recording()?;
            std::thread::sleep(std::time::Duration::from_millis(40));
            let audio = AudioRaw::new_raw(rec_dev.read()?.expect("No audio").to_owned(), DEFAULT_SAMPLES_PER_SECOND);
            rec_dev.stop_recording()?;
            let opus = audio.to_ogg_opus()?;
            let audio2 = decode_ogg_opus(opus, DEFAULT_SAMPLES_PER_SECOND)?;
            assert_eq!(audio.len(), audio2.0.len());
            Ok(())
        }
    }
}
