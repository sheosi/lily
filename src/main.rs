mod stt;
mod tts;
mod audio;
mod gtts;
mod nlu;
mod vars;
mod hotword;
mod python;
mod packages;
mod vad;
mod extensions;
mod config;
mod path_ext;

// Standard library
use std::path::Path;
use std::cell::RefCell;

// This crate
use crate::audio::{RecDevice, PlayDevice, Recording};
use crate::hotword::{HotwordDetector, Snowboy};
use crate::nlu::{Nlu, NluFactory};
use crate::vars::*;
use crate::packages::load_packages;
use crate::python::python_init;
use crate::tts::{Gender, VoiceDescr};

// Other crates
use log::{info, warn};
use unic_langid::LanguageIdentifier;
use anyhow::{anyhow, Result};
use cpython::PyDict;
use snips_nlu_ontology::Slot;

// To be shared on the same thread
thread_local! {
    static TTS: RefCell<Box<dyn crate::tts::Tts>> = RefCell::new(tts::TtsFactory::dummy());
}

enum ProgState {
    WaitingForHotword,
    Listening,
}

fn add_slots(base_context: &PyDict, slots: Vec<Slot>) -> Result<PyDict> {
    let gil = cpython::Python::acquire_gil();
    let py = gil.python();

    // What to do here if this fails?
    let result = base_context.copy(py).map_err(|py_err|anyhow!("Python error while copying base context: {:?}", py_err))?;

    for slot in slots.into_iter() {
        result.set_item(py, slot.slot_name, slot.raw_value).map_err(
            |py_err|anyhow!("Couldn't set name in base context: {:?}", py_err)
        )?;
    }

    Ok(result)

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

// Main loop, waits for hotword then records, acts and starts agian
fn record_loop() -> Result<()> {
    // Set language
    let config = crate::config::get_conf();
    println!("{:?}", config);
    let curr_lang : LanguageIdentifier = get_locale_default().parse().expect("Locale parsing failed");
    {
        let gil = cpython::Python::acquire_gil();
        let py = gil.python();

        crate::python::set_python_locale(py, &curr_lang)?;
    }
    let ibm_tts_gateway_key = config.extract_ibm_tts_data();
    let ibm_stt_gateway_key = config.extract_ibm_stt_data();

    let mut order_map = load_packages(&Path::new(&PACKAGES_PATH.resolve()), &curr_lang)?;

    const VOICE_PREFS: VoiceDescr = VoiceDescr {gender: Gender::Female};
    let new_tts = tts::TtsFactory::load_with_prefs(&curr_lang, config.prefer_online_tts, ibm_tts_gateway_key.clone(), &VOICE_PREFS)?;
    TTS.with(|a|(&a).replace(new_tts));
    info!("Using tts {}", TTS.with(|a|(*a).borrow().get_info()));

    let mut record_device = RecDevice::new()?;
    let mut _play_device = PlayDevice::new();


    let mut stt = stt::SttFactory::load(&curr_lang, config.prefer_online_stt, ibm_stt_gateway_key)?;
    info!("Using stt {}", stt.get_info());

    let mut hotword_detector = {
        let snowboy_path = SNOWBOY_DATA_PATH.resolve();
        Snowboy::new(&snowboy_path.join("lily.pmdl"), &snowboy_path.join("common.res"), config.hotword_sensitivity)?
    };

    info!("Init Nlu");
    let nlu = NluFactory::new_nlu(&NLU_ENGINE_PATH.resolve())?;
    let mut current_state = ProgState::WaitingForHotword;


    let base_context = {
        let gil = cpython::Python::acquire_gil();
        let py = gil.python();

        PyDict::new(py)
    };

    order_map.call_order("lily_start", &base_context)?;
    

    let mut current_speech = crate::audio::Audio::new_empty(DEFAULT_SAMPLES_PER_SECOND);

    info!("Start Recording");
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
                        stt.begin_decoding()?;
                        info!("Hotword detected");
                        order_map.call_order("init_reco", &base_context)?;
                        record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);
                    }
                    _ => {}
                }
            }
            ProgState::Listening => {
                current_speech.append_raw(microphone_data, DEFAULT_SAMPLES_PER_SECOND);

                match stt.decode(microphone_data)? {
                    stt::DecodeState::NotStarted => {},
                    stt::DecodeState::StartListening => {
                        info!("Listening speech");
                    }
                    stt::DecodeState::NotFinished => {}
                    stt::DecodeState::Finished(decode_res) => {
                        info!("End of speech");
                        current_state = ProgState::WaitingForHotword;
                        match decode_res {
                            None => warn!("Not recognized"),
                            Some((hypothesis, _utt_id, _score)) => {
                                record_device.stop_recording().expect(AUDIO_REC_STOP_ERR_MSG);

                                if !hypothesis.is_empty() {
                                    
                                    let result = nlu.parse(&hypothesis).map_err(|err|anyhow!("Failed to parse: {:?}", err))?;
                                    info!("{:?}", result);
                                    let score = result.intent.confidence_score;
                                    info!("Score: {}",score);

                                    // Do action if at least we are 80% confident on
                                    // what we got
                                    if score >= MIN_SCORE_FOR_ACTION {
                                        info!("Let's call an action");
                                        if let Some(intent_name) = result.intent.intent_name {
                                            let slots_context = add_slots(&base_context,result.slots)?;
                                            order_map.call_order(&intent_name, &slots_context)?;
                                            info!("Action called");
                                        }
                                        else {
                                            order_map.call_order("unrecognized", &base_context)?;
                                        }
                                    }
                                    else {
                                        order_map.call_order("unrecognized", &base_context)?;
                                    }
                                    record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);
                                    save_recording_to_disk(&mut current_speech, LAST_SPEECH_PATH.resolve().as_path());
                                    current_speech.clear();
                                }
                                else {
                                    order_map.call_order("empty_reco", &base_context)?;
                                }

                                hotword_detector.start_hotword_check()?;
                            }   
                        }
                    }
                }
            }
        }
    }

    //Ok(())
}

fn init_log() {
    let formatter = syslog::Formatter3164 {
        facility: syslog::Facility::LOG_USER,
        hostname: None,
        process: "lily".into(),
        pid: 0,
    };


    let log_level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    }
    else {
        log::LevelFilter::Info
    };

    let logger = syslog::unix(formatter).expect("could not connect to syslog");
    log::set_boxed_logger(Box::new(syslog::BasicLogger::new(logger)))
            .map(|()| log::set_max_level(log_level)).ok();
            //simple_logger::init()?;

}

fn get_locale_default() -> String {
    for (tag, val) in locale_config::Locale::user_default().tags() {
        if let None = tag {
            return format!("{}", val)
        }
    }

    "".to_string()
}

fn main() -> Result<()> {
    ctrlc::set_handler(move || {
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");

    init_log();
    python_init()?;
    record_loop()?;

    Ok(())
}