// Standard library
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::TryInto;
use std::mem;
use std::path::Path;

// This crate
use crate::nlu::{Nlu, SnipsNlu, SnipsNluManager, NluManager, NluUtterance, EntityInstance, EntityDef, EntityData};
use crate::python::try_translate_all;
use crate::extensions::{OrderMap, ActionSet};
use crate::vars::{NLU_TRAIN_SET_PATH, NLU_ENGINE_PATH, MIN_SCORE_FOR_ACTION};
use crate::tts::{VoiceDescr, Gender, TtsFactory};
use crate::config::Config;
use crate::audio::{PlayDevice, RecDevice, Recording};
use crate::vars::*;
use crate::stt::{SttFactory, DecodeState};
use crate::hotword::{HotwordDetector, Snowboy};

// Other crates
use unic_langid::LanguageIdentifier;
use serde::Deserialize;
use log::{info, warn};
use anyhow::{Result, anyhow};
use snips_nlu_ontology::Slot;
use cpython::PyDict;
#[derive(Deserialize)]
struct YamlEntityDef {
    data: Vec<String>
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OrderKind {
    Ref(String),
    Def(YamlEntityDef)
}

#[derive(Deserialize)]
struct OrderEntity {
    kind: OrderKind,
    example: String
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OrderData {
    Direct(String),
    WithEntities{text: String, entities: HashMap<String, OrderEntity>}
}
// To be shared on the same thread
thread_local! {
    pub static TTS: RefCell<Box<dyn crate::tts::Tts>> = RefCell::new(TtsFactory::dummy());
}

pub fn add_order<N: NluManager>(
    sig_arg: serde_yaml::Value,
    nlu_man: &mut N,
    skill_name: &str,
    pkg_name: &str,
) -> Result<()> {
    let ord_data: OrderData = serde_yaml::from_value(sig_arg)?;
    match ord_data {
        OrderData::Direct(order_str) => {
            // into_iter lets us do a move operation
            let utts = try_translate_all(&order_str)?
                .into_iter()
                .map(|utt| NluUtterance::Direct(utt))
                .collect();
            nlu_man.add_intent(skill_name, utts);
        }
        OrderData::WithEntities { text, entities } => {
            let mut entities_res = HashMap::new();
            for (ent_name, ent_data) in entities.into_iter() {
                let ent_kind_name = match ent_data.kind {
                    OrderKind::Ref(name) => name,
                    OrderKind::Def(def) => {
                        let name = format!("_{}__{}_", pkg_name, ent_name);
                        nlu_man.add_entity(&name, def.try_into()?);
                        name
                    }
                };
                entities_res.insert(
                    ent_name,
                    EntityInstance {
                        kind: ent_kind_name,
                        example: ent_data.example,
                    },
                );
            }
            let utts = try_translate_all(&text)?
                .into_iter()
                .map(|utt| NluUtterance::WithEntities {
                    text: utt,
                    entities: entities_res.clone(),
                })
                .collect();
            nlu_man.add_intent(skill_name, utts);
        }
    }
    Ok(())
}

// Answers to a user order (either by voice or by text)
pub struct SignalOrder {
    order_map: OrderMap,
    nlu_man: Option<SnipsNluManager>,
    nlu: Option<SnipsNlu>
}

impl SignalOrder {
    pub fn new() -> Self {
        SignalOrder {
            order_map: OrderMap::new(),
            nlu_man: Some(SnipsNluManager::new()),
            nlu: None
        }
    }
    
    pub fn add(&mut self, sig_arg: serde_yaml::Value, skill_name: &str, pkg_name: &str, act_set: Rc<RefCell<ActionSet>>) -> Result<()> {
        match self.nlu_man {
            Some(ref mut nlu_man) => {
                add_order(sig_arg, nlu_man, skill_name, pkg_name)?;
                self.order_map.add_order(skill_name, act_set);

                Ok(())
            }
            None => {
                panic!("Called add_order after end_loading");
            }
        }
    }

    pub fn end_loading(&mut self, curr_lang: &LanguageIdentifier) -> Result<()> {
        let res = match mem::replace(&mut self.nlu_man, None) {
            Some(nlu_man) => {
                nlu_man.train(&NLU_TRAIN_SET_PATH.resolve(), &NLU_ENGINE_PATH.resolve(), curr_lang)
            }
            None => {
                panic!("Called end_loading twice");
            }
        };

        info!("Init Nlu");
        self.nlu = Some(SnipsNlu::new(&NLU_ENGINE_PATH.resolve())?);
        
        res
    }

    fn received_order(&mut self, decode_res: Option<(String, Option<String>, i32)>, event_signal: &mut SignalEvent, base_context: &PyDict) -> Result<()> {
        match decode_res {
            None => warn!("Not recognized"),
            Some((hypothesis, _utt_id, _score)) => {
                

                if !hypothesis.is_empty() {
                    match self.nlu {
                        Some(ref mut nlu) => {
                            let result = nlu.parse(&hypothesis).map_err(|err|anyhow!("Failed to parse: {:?}", err))?;
                            info!("{:?}", result);
                            let score = result.intent.confidence_score;
                            info!("Score: {}",score);

                            // Do action if at least we are 80% confident on
                            // what we got
                            if score >= MIN_SCORE_FOR_ACTION {
                                info!("Let's call an action");
                                if let Some(intent_name) = result.intent.intent_name {
                                    let slots_context = add_slots(base_context,result.slots)?;
                                    self.order_map.call_order(&intent_name, &slots_context)?;
                                    info!("Action called");
                                }
                                else {
                                    event_signal.call("unrecognized", &base_context)?;
                                }
                            }
                            else {
                                event_signal.call("unrecognized", &base_context)?;
                            }
                            
                        },
                        None => {
                            panic!("received_order can't be called before end_loading")
                        }
                    }
                }
                else {
                    event_signal.call("empty_reco", &base_context)?;
                }
            }
        }
    Ok(())
    }

    pub fn record_loop(&mut self, signal_event: &mut SignalEvent, config: &Config, base_context: &PyDict, curr_lang: &LanguageIdentifier) -> Result<()> {
        let ibm_tts_gateway_key = config.extract_ibm_tts_data();
        let ibm_stt_gateway_key = config.extract_ibm_stt_data();

        const VOICE_PREFS: VoiceDescr = VoiceDescr {gender: Gender::Female};
        let new_tts = TtsFactory::load_with_prefs(&curr_lang, config.prefer_online_tts, ibm_tts_gateway_key.clone(), &VOICE_PREFS)?;
        TTS.with(|a|(&a).replace(new_tts));
        info!("Using tts {}", TTS.with(|a|(*a).borrow().get_info()));

        let mut record_device = RecDevice::new()?;
        let mut _play_device = PlayDevice::new();

        let mut stt = SttFactory::load(&curr_lang, config.prefer_online_stt, ibm_stt_gateway_key)?;
        info!("Using stt {}", stt.get_info());

        let mut hotword_detector = {
            let snowboy_path = SNOWBOY_DATA_PATH.resolve();
            Snowboy::new(&snowboy_path.join("lily.pmdl"), &snowboy_path.join("common.res"), config.hotword_sensitivity)?
        };
        signal_event.call("lily_start", &base_context)?;

            let mut current_speech = crate::audio::Audio::new_empty(DEFAULT_SAMPLES_PER_SECOND);
        let mut current_state = ProgState::WaitingForHotword;
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
                            signal_event.call("init_reco", &base_context)?;
                            record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);
                        }
                        _ => {}
                    }
                }
                ProgState::Listening => {
                    current_speech.append_raw(microphone_data, DEFAULT_SAMPLES_PER_SECOND);

                    match stt.decode(microphone_data)? {
                        DecodeState::NotStarted => {},
                        DecodeState::StartListening => {
                            info!("Listening speech");
                        }
                        DecodeState::NotFinished => {}
                        DecodeState::Finished(decode_res) => {
                            info!("End of speech");
                            current_state = ProgState::WaitingForHotword;
                            record_device.stop_recording().expect(AUDIO_REC_STOP_ERR_MSG);
                            self.received_order(decode_res, signal_event, &base_context)?;
                            record_device.start_recording().expect(AUDIO_REC_START_ERR_MSG);
                            save_recording_to_disk(&mut current_speech, LAST_SPEECH_PATH.resolve().as_path());
                            current_speech.clear();
                            hotword_detector.start_hotword_check()?;
                        }
                    }
                }
            }use std::cell::RefCell;
use std::collections::HashMap;l();
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

// A especial signal to be called by the system whenever something happens
pub struct SignalEvent {
    event_map: OrderMap
}

impl SignalEvent {
    pub fn new() -> Self {
        Self {event_map: OrderMap::new()}
    }

    pub fn add(&mut self, event_name: &str, act_set: Rc<RefCell<ActionSet>>) {
        self.event_map.add_order(event_name, act_set)
    }

    pub fn call(&mut self, event_name: &str, context: &PyDict) -> Result<()> {
        self.event_map.call_order(event_name, context)
    }
}

impl TryInto<EntityDef> for YamlEntityDef {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<EntityDef> {
        let mut data = Vec::new();

        for trans_data in self.data.into_iter(){
            let mut translations = try_translate_all(&trans_data)?;
            let value = translations.swap_remove(0);
            data.push(EntityData{value, synonyms: translations});
        }

        Ok(EntityDef{data, use_synonyms: true, automatically_extensible: true})
    }
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
