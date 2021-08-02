// Standard library
use std::io::Cursor;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};

// This crate
use crate::{actions::ActionContext, stt::DecodeRes};
use crate::config::Config;
use crate::exts::LockIt;
use crate::nlu::{NluManager, NluManagerStatic};
use crate::signals::{dev_mgmt::SessionManager, process_answers, SignalEventShared, SignalOrder};
use crate::stt::{SttPool, SttSet};

// Other crates
use anyhow::Result;
use lily_common::audio::{Audio, AudioRaw};
use lily_common::communication::*;
use lily_common::vars::{DEFAULT_SAMPLES_PER_SECOND, PathRef};
use log::{error, warn};
use ogg_opus::decode as opus_decode;
use tokio::sync::mpsc;
use unic_langid::LanguageIdentifier;

mod language_detection {
    // use unic_langid::LanguageIdentifier;
    //use lingua::{Language, LanguageDetector, LanguageDetectorBuilder};

    /*fn id_to_lingua() -> lingua::La{

    }*/

    /*let languages = vec![English, French, German, Spanish];
    let detector: LanguageDetector = LanguageDetectorBuilder::from_languages(&languages).build();
    let detected_language: Option<Language> = detector.detect_language_of("languages are awesome");*/
}

/*** Reactions ****************************************************************/

pub async fn on_nlu_request<M: NluManager + NluManagerStatic + Debug + Send + 'static>(
    config: &Config,
    mut channel: mpsc::Receiver<MsgRequest>,
    signal_event: SignalEventShared,
    curr_langs: &Vec<LanguageIdentifier>,
    order: &mut SignalOrder<M>,
    base_context: &ActionContext,
    sessions: Arc<Mutex<SessionManager>>
) -> Result<()> {
    let mut stt_set = SttSet::new();
    let ibm_data = config.stt.ibm.clone();
    for lang in curr_langs {
        let pool= SttPool::new(1, 1,lang, config.stt.prefer_online, &ibm_data).await?;
        stt_set.add_lang(lang, pool).await?;
    }

    let mut stt_audio = AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND);
    let audio_debug_path = PathRef::user_cfg("stt_audio.ogg").resolve();

    loop {
        let msg_nlu = channel.recv().await.expect("Channel closed!");
        match msg_nlu.data {
            RequestData::Text(text) => {
                let lang = &curr_langs[0];
                let context = add_context_data(&base_context, lang, &msg_nlu.satellite);
                let decoded = Some(DecodeRes{hypothesis: text, confidence: 1.0});
                

                do_received_order(
                    order,
                    decoded,
                    signal_event.clone(),
                    &context,
                    lang,
                    msg_nlu.satellite,
                    &sessions
                ).await;
            }
            RequestData::Audio{data: audio, is_final} => {
                let (as_raw, _) = opus_decode::<_, DEFAULT_SAMPLES_PER_SECOND>(Cursor::new(audio))?;

                if cfg!(debug_assertions) {
                    stt_audio.append_audio(&as_raw, DEFAULT_SAMPLES_PER_SECOND)?;
                }

                let session = sessions
                .lock_it()
                .session_for(msg_nlu.satellite.clone())
                .upgrade()
                .expect("Session has been deleted right now?");

                {
                    match session.lock_it().get_stt_or_make(&mut stt_set, &as_raw).await {
                        Ok(stt) => {
                            if let Err(e) = stt.process(&as_raw).await {
                                error!("Stt failed to process audio: {}", e);
                            }

                            else if is_final {
                                if cfg!(debug_assertions) {
                                    stt_audio.save_to_disk(&audio_debug_path)?;
                                    stt_audio.clear();
                                }
                                let satellite = msg_nlu.satellite.clone();
                                let context = add_context_data(&base_context, stt.lang(), &satellite);
                                match stt.end_decoding().await {
                                    Ok(decoded)=> {
                                        do_received_order(
                                            order,
                                            decoded,
                                            signal_event.clone(),
                                            &context,
                                            stt.lang(),
                                            msg_nlu.satellite.clone(),
                                            &sessions
                                        ).await;
                                    }
                                    Err(e) => error!("Stt failed while doing final decode: {}", e)
                                }
                                
                            }
                        }
                        Err(e) => {
                            error!("Failed to obtain Stt for this session: {}", e);
                        }
                    }
                }
                if is_final {
                    if let Err(e) = session.lock_it().end_utt() {
                        warn!("{}",e);
                    }
                }
            }
        }
    }
}

pub async fn on_event(
    mut channel: mpsc::Receiver<MsgEvent>,
    signal_event: SignalEventShared,
    def_lang: Option<&LanguageIdentifier>,
    base_context: &ActionContext,
) -> Result<()> {
    let def_lang = def_lang.unwrap();
    loop {
        let msg = channel.recv().await.expect("Channel closed!");
        let context = add_context_data(base_context, &def_lang, &msg.satellite);
        let ans = signal_event.lock_it().call(&msg.event, context.clone());
        if let Err(e) = process_answers(ans,&def_lang,msg.satellite) {
            error!("Occurred a problem while processing event: {}", e);
        }
    }
}


async fn do_received_order<M: NluManager + NluManagerStatic + Debug + Send + 'static>(
    order: &mut SignalOrder<M>,
    decoded: Option<DecodeRes>,
    signal_event: SignalEventShared,
    context: &ActionContext,
    lang: &LanguageIdentifier,
    satellite: String,
    sessions: &Arc<Mutex<SessionManager>>
) {
    match order.received_order(
        decoded, 
        signal_event,
        context,
        lang,
        satellite.clone()
    ).await {
        Ok(s_end) => {
            if s_end {
                if let Err(e) = sessions.lock_it().end_session(&satellite) {
                    error!("Failed to end session for {}: {}", &satellite, e);
                }
            }
        }
        Err(e) => {error!("Actions processing had an error: {}", e)}
    }
}

pub fn add_context_data(dict: &ActionContext, lang: &LanguageIdentifier, client: &str) -> ActionContext {
    
    let mut new = dict.copy();
    new.set_str("locale".into(), lang.to_string());
    let mut satellite = ActionContext::new();
    satellite.set_str("uuid".to_string(), client.to_string());
    new.set_dict("satellite".into(), satellite);

    new
}

#[derive(Debug)]
pub enum SendData {
    String((String, LanguageIdentifier)),
    Audio(Audio)
}

