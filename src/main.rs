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

// Standard library
use std::path::Path;

// This crate
use crate::audio::{RecDevice, PlayDevice, Recording};
use crate::hotword::{HotwordDetector, Snowboy};
use crate::nlu::Nlu;
use crate::vars::*;
use crate::packages::load_packages;
use crate::python::python_init;

// Other crates
use log::{info, warn};
use ref_thread_local::{RefThreadLocal, ref_thread_local};
use unic_langid::LanguageIdentifier;
use serde::Deserialize;

// To be shared on the same thread
ref_thread_local! {
    static managed TTS: Box<dyn crate::tts::Tts> = tts::TtsFactory::dummy();
}

enum ProgState {
    WaitingForHotword,
    Listening,
}

#[derive(Deserialize)]
struct Config {
    #[serde(default = "false_val")]
    prefer_online_tts: bool,
    #[serde(default = "false_val")]
    prefer_online_stt: bool,
    #[serde(default = "none_str")]
    ibm_tts_key: Option<String>,
    #[serde(default = "none_str")]
    ibm_stt_key: Option<String>,
    #[serde(default = "none_str")]
    ibm_gateway: Option<String>
}

fn false_val() -> bool {
    false
}

fn none_str() -> Option<String> {
    None
}

fn load_conf() -> Option<Config> {
    let conf_path = resolve_path(MAIN_CONF_PATH);
    if conf_path.is_file() {
        let conf_file = std::fs::File::open(conf_path).unwrap();
        Some(serde_yaml::from_reader(std::io::BufReader::new(conf_file)).unwrap())
    }
    else {
        None
    }

}


// Main loop, waits for hotword then records, acts and starts agian
fn record_loop() {
    // Set language
    let config = load_conf().unwrap_or(Config{prefer_online_tts: false, prefer_online_stt: false, ibm_tts_key: None, ibm_stt_key: None, ibm_gateway: None});
    let curr_lang : LanguageIdentifier = get_locale_default().parse().expect("Locale parsing failed");
    let ibm_tts_gateway_key = {
        if config.ibm_gateway.is_some() && config.ibm_tts_key.is_some() {
            Some((config.ibm_gateway.clone().unwrap(), config.ibm_tts_key.unwrap().clone()))
        }
        else {
            None
        }
    };
    let ibm_stt_gateway_key = {
        if config.ibm_gateway.is_some() && config.ibm_stt_key.is_some() {
            Some((config.ibm_gateway.unwrap().clone(), config.ibm_stt_key.unwrap().clone()))
        }
        else {
            None
        }
    };

    let mut order_map = load_packages(&Path::new(&resolve_path(PACKAGES_PATH)), &curr_lang).unwrap();
    *TTS.borrow_mut() = tts::TtsFactory::load(&curr_lang, config.prefer_online_tts, ibm_tts_gateway_key.clone());
    info!("Using tts {}", TTS.borrow().get_info());

    let mut record_device = RecDevice::new().unwrap();
    let mut _play_device = PlayDevice::new();



    let mut stt = stt::SttFactory::load(&curr_lang, config.prefer_online_stt, ibm_stt_gateway_key);
    info!("Using stt {}", stt.get_info());

    let mut hotword_detector = {
        let snowboy_path = resolve_path(SNOWBOY_DATA_PATH);
        Snowboy::new(&snowboy_path.join("lily.pmdl"), &snowboy_path.join("common.res"))
    };

    info!("Init Nlu");
    let nlu = Nlu::new(&resolve_path(NLU_ENGINE_PATH));
    let mut current_state = ProgState::WaitingForHotword;


    info!("Record start");

    // Start recording
    record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);
    hotword_detector.start_hotword_check();

    order_map.call_order("lily_start").unwrap();

    let mut current_speech = crate::audio::Audio{buffer: Vec::new(), samples_per_second: 16000};

    loop {
        let microphone_data = match record_device.read().unwrap() {
            Some(d) => d,
            None => continue,
        };

        match current_state {
            ProgState::WaitingForHotword => {
                match hotword_detector.check_hotword(microphone_data) {
                    true => {
                        // Don't record for a moment
                        record_device.stop_recording().expect(AUDIO_REC_STOP_ERR_MSG);
                        current_state = ProgState::Listening;
                        stt.begin_decoding().unwrap();
                        info!("Hotword detected");
                        order_map.call_order("init_reco").unwrap();
                        record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);
                    }
                    _ => {}
                }
            }
            ProgState::Listening => {
                current_speech.append_audio(microphone_data, 16000);

                match stt.decode(microphone_data).unwrap() {
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
                                if !hypothesis.is_empty() {
                                    record_device.stop_recording().expect(AUDIO_REC_STOP_ERR_MSG);
                                    /*for seg in ps_decoder.seg_iter() {
                                        println!("{} : {}, {}",seg.word(), seg.prob().ascr, seg.prob().lscr);

                                    }*/
                                    let result = nlu.parse(&hypothesis).unwrap();
                                    let result_json = Nlu::to_json(&result);
                                    info!("{}", result_json);
                                    let score = result.intent.confidence_score;
                                    info!("Score: {}",score);

                                    // Do action if at least we are 80% confident on
                                    // what we got
                                    if score >= 0.55 {
                                        info!("Let's call an action");
                                        if let Some(intent_name) = result.intent.intent_name {
                                            order_map.call_order(&intent_name).unwrap();
                                            info!("Action called");
                                        }
                                        else {
                                            order_map.call_order("unrecognized").unwrap();
                                        }
                                    }
                                    else {
                                        order_map.call_order("unrecognized").unwrap();
                                    }
                                    record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);
                                    hotword_detector.start_hotword_check();
                                    current_speech.write_wav(resolve_path(LAST_SPEECH_PATH).to_str().unwrap()).unwrap();
                                    current_speech.clear();
                                }
                                else {
                                    order_map.call_order("empty_reco").unwrap();
                                }
                            }   
                        }
                    }
                }
            }
        }
    }
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
            //simple_logger::init().unwrap();

}

fn get_locale_default() -> String {
    for (tag, val) in locale_config::Locale::user_default().tags() {
        if let None = tag {
            return format!("{}", val)
        }
    }

    "".to_string()
}

fn main() {
    init_log();
    python_init();
    record_loop();
}