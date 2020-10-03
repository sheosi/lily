use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread::sleep;

use crate::audio::{AudioRaw, PlayDevice, Recording, RecDevice};
use crate::config::Config;
use crate::hotword::{HotwordDetector, Snowboy};
use crate::interfaces::{SharedOutput, UserInterface, UserInterfaceOutput};
use crate::signals::SignalEventShared;
use crate::stt::{DecodeRes, DecodeState, SttFactory, SttStream};
use crate::tts::{VoiceDescr, Gender, Tts, TtsFactory};
use crate::vars::*;

use anyhow::Result;
use pyo3::{types::PyDict, Py};
use log::{info, warn};
use unic_langid::LanguageIdentifier;

#[derive(PartialEq)]
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
    stt: Box<dyn SttStream>,
    output: Arc<Mutex<DirectVoiceInterfaceOutput>>,

}

const ENERGY_SAMPLING_TIME_MS: u64 = 500;
impl DirectVoiceInterface {
    pub fn new(curr_lang: &LanguageIdentifier, config: &Config) -> Result<Self> {
        let ibm_stt_gateway_key = config.extract_ibm_stt_data();

        // Record environment to get minimal energy threshold
        let mut record_device = RecDevice::new()?;
        record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);
        sleep(Duration::from_millis(ENERGY_SAMPLING_TIME_MS));
        let audio_sample = {
            match record_device.read()? {
                Some(buffer) => {
                    AudioRaw::new_raw(buffer.to_owned(), DEFAULT_SAMPLES_PER_SECOND)
                }
                None => {
                    warn!("Couldn't obtain mic input data for energy sampling while loading");
                    AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND)
                }
            }

        };

        record_device.stop_recording()?;

        let stt = SttFactory::load(curr_lang, &audio_sample,  config.prefer_online_stt, ibm_stt_gateway_key)?;
        info!("Using stt {}", stt.get_info());
        
        let output_obj = DirectVoiceInterfaceOutput::new(curr_lang, config)?;
        let output = Arc::new(Mutex::new(output_obj));

        Ok(DirectVoiceInterface{stt, output})
    }

    pub fn interface_loop<F: FnMut( Option<DecodeRes>, SignalEventShared)->Result<()>> (&mut self, config: &Config, signal_event: SignalEventShared, base_context: &Py<PyDict>, mut callback: F) -> Result<()> {
        let mut record_device = RecDevice::new()?;
        let mut _play_device = PlayDevice::new();

        let mut current_speech = if config.debug_record_active_speech{
            Some(crate::audio::Audio::new_empty(DEFAULT_SAMPLES_PER_SECOND))
        }
        else {
            None
        };

        let mut current_state = ProgState::WaitingForHotword;
        let mut hotword_detector = {
            let snowboy_path = SNOWBOY_DATA_PATH.resolve();
            Snowboy::new(&snowboy_path.join("lily.pmdl"), &snowboy_path.join("common.res"), config.hotword_sensitivity)?
        };
        info!("Start Recording");
        signal_event.borrow_mut().call("lily_start", &base_context)?;
        // Start recording
        record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);
        hotword_detector.start_hotword_check()?; 

            loop {
                let interval =
                    if current_state == ProgState::WaitingForHotword {HOTWORD_CHECK_INTERVAL_MS}
                    else {ACTIVE_LISTENING_INTERVAL_MS};

                let microphone_data = match record_device.read_for_ms(interval)? {
                    Some(d) => d,
                    None => continue,
                };


            match current_state {
                ProgState::WaitingForHotword => {
                    match hotword_detector.check_hotword(microphone_data)? {
                        true => {
                            current_state = ProgState::Listening;
                            signal_event.borrow_mut().call("init_reco", &base_context)?;
                            info!("Hotword detected");
                            // This could take a while 
                            self.stt.begin_decoding()?;
                        }
                        _ => {}
                    }
                }
                ProgState::Listening => {
                    if let Some(ref mut curr) = current_speech {
                        curr.append_raw(microphone_data, DEFAULT_SAMPLES_PER_SECOND);
                    }
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

                            info!("{:?}", decode_res);
                            callback(decode_res, signal_event.clone())?;
                            record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);

                            if let Some(ref mut curr) = current_speech {
                                save_recording_to_disk(curr, LAST_SPEECH_PATH.resolve().as_path());
                                curr.clear();
                            }

                            hotword_detector.start_hotword_check()?;

                        }
                    }
                }
            }
        }
    }
}


struct DirectVoiceInterfaceOutput {
    tts: Box<dyn Tts>
}

impl DirectVoiceInterfaceOutput {
    fn new(curr_lang: &LanguageIdentifier, config: &Config) -> Result<Self> {
        const VOICE_PREFS: VoiceDescr = VoiceDescr {gender: Gender::Female};
        let ibm_tts_gateway_key = config.extract_ibm_tts_data();

        let tts = TtsFactory::load_with_prefs(curr_lang, config.prefer_online_tts, ibm_tts_gateway_key.clone(), &VOICE_PREFS)?;
        info!("Using tts {}", tts.get_info());

        Ok(Self{tts})
    }
}

impl UserInterfaceOutput for DirectVoiceInterfaceOutput {
    fn answer(&mut self, input: &str) -> Result<()> {
        let audio = self.tts.synth_text(input)?;
        PlayDevice::new()?.wait_audio(audio)?;
        Ok(())
    }
}



impl UserInterface for DirectVoiceInterface {
    fn get_output(&self) -> SharedOutput {
        self.output.clone()
    }
}