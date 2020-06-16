use std::cell::RefCell;
use std::path::Path;

use crate::audio::{PlayDevice, RecDevice, Recording};
use crate::config::Config;
use crate::hotword::{HotwordDetector, Snowboy};
use crate::signals::SignalEvent;
use crate::stt::{SttFactory, DecodeState, SttStream};
use crate::tts::{VoiceDescr, Gender, TtsFactory};
use crate::vars::*;

use anyhow::Result;
use cpython::PyDict;
use log::{info, warn};
use unic_langid::LanguageIdentifier;

// To be shared on the same thread
thread_local! {
    pub static TTS: RefCell<Box<dyn crate::tts::Tts>> = RefCell::new(TtsFactory::dummy());
}

enum ProgState {
    WaitingForHotword,
    Listening,
}

fn save_recording_to_disk(recording: &mut crate::audio::Audio, path: &Path) {
    if let Some(str_path) = path.to_str() {
        if let Err(err) = recording.write_ogg(str_path) {
            warn!("Couldn't save recording: {:?}", err);
        }
    }
    else {
        warn!("Couldn't save recording, failed to transform path to unicode: {:?}", path);
    }
}

pub struct DirectVoiceInterface {
    stt: Box<dyn SttStream>
}

impl DirectVoiceInterface {
    pub fn new(curr_lang: &LanguageIdentifier, config: &Config) -> Result<Self> {
        let ibm_tts_gateway_key = config.extract_ibm_tts_data();
        let ibm_stt_gateway_key = config.extract_ibm_stt_data();

        const VOICE_PREFS: VoiceDescr = VoiceDescr {gender: Gender::Female};
        let new_tts = TtsFactory::load_with_prefs(curr_lang, config.prefer_online_tts, ibm_tts_gateway_key.clone(), &VOICE_PREFS)?;
        TTS.with(|a|(&a).replace(new_tts));
        info!("Using tts {}", TTS.with(|a|(*a).borrow().get_info()));

        let stt = SttFactory::load(curr_lang, config.prefer_online_stt, ibm_stt_gateway_key)?;
        info!("Using stt {}", stt.get_info());
        
        Ok(DirectVoiceInterface{stt})
    }

    pub fn interface_loop<F: FnMut( Option<(String, Option<String>, i32)>, &mut SignalEvent)->Result<()>> (&mut self, config: &Config, signal_event: &mut SignalEvent, base_context: &PyDict, mut callback: F) -> Result<()> {
        let mut record_device = RecDevice::new()?;
        let mut _play_device = PlayDevice::new();

        let mut current_speech = crate::audio::Audio::new_empty(DEFAULT_SAMPLES_PER_SECOND);
        let mut current_state = ProgState::WaitingForHotword;
        let mut hotword_detector = {
            let snowboy_path = SNOWBOY_DATA_PATH.resolve();
            Snowboy::new(&snowboy_path.join("lily.pmdl"), &snowboy_path.join("common.res"), config.hotword_sensitivity)?
        };
        info!("Start Recording");
        signal_event.call("lily_start", &base_context)?;
        // Start recording
        record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);
        hotword_detector.start_hotword_check()?; 

           loop {
            let microphone_data = match record_device.read_for_ms(HOTWORD_CHECK_INTERVAL_MS)? {
                Some(d) => d,
                None => continue,
            };

            match current_state {
                ProgState::WaitingForHotword => {
                    match hotword_detector.check_hotword(microphone_data)? {
                        true => {
                            // Don't record for a moment
                            record_device.stop_recording().expect(AUDIO_REC_STOP_ERR_MSG);
                            current_state = ProgState::Listening;
                            self.stt.begin_decoding()?;
                            info!("Hotword detected");
                            signal_event.call("init_reco", &base_context)?;
                            record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);
                        }
                        _ => {}
                    }
                }
                ProgState::Listening => {
                    current_speech.append_raw(microphone_data, DEFAULT_SAMPLES_PER_SECOND);

                    match self.stt.decode(microphone_data)? {
                        DecodeState::NotStarted => {},
                        DecodeState::StartListening => {
                            info!("Listening speech");
                        }
                        DecodeState::NotFinished => {}
                        DecodeState::Finished(decode_res) => {
                            info!("End of speech");
                            current_state = ProgState::WaitingForHotword;
                            record_device.stop_recording().expect(AUDIO_REC_STOP_ERR_MSG);
                            //self.received_order(decode_res, signal_event, &base_context)?;
                            callback(decode_res, signal_event)?;
                            record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);
                            save_recording_to_disk(&mut current_speech, LAST_SPEECH_PATH.resolve().as_path());
                            current_speech.clear();
                            hotword_detector.start_hotword_check()?;
                        }
                    }
                }
            }
        }
    }
}