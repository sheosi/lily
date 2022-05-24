//! Implements espeak TTS, note because of how espeak works there can only be
//! one instance of EspeakTTS in the whole program
use core::convert::TryInto;
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::ptr::null;
use std::sync::Mutex;

use crate::tts::{Gender, Tts, TtsConstructionError, TtsError, TtsInfo, TtsStatic, VoiceDescr};

use async_trait::async_trait;
use espeak_ng_sys::*;
use lazy_static::lazy_static;
use libc::{c_int, c_short};
use lily_common::audio::Audio;
use lily_common::vars::DEFAULT_SAMPLES_PER_SECOND;
use log::warn;
use unic_langid::LanguageIdentifier;

lazy_static! {
    static ref SOUND_BUFFER: Mutex<RefCell<Vec<i16>>> = Mutex::new(RefCell::new(Vec::new()));
}

pub struct EspeakTts {}

unsafe fn from_lang_and_prefs(lang: &LanguageIdentifier, prefs: &VoiceDescr) -> *mut espeak_VOICE {
    let lang = CString::new(lang.language.as_str())
        .expect("Language name had some internal nul character");
    let gender = match prefs.gender {
        Gender::Male => 1,
        Gender::Female => 2,
    };

    let mut descr = espeak_GetCurrentVoice();
    (*descr).name = null();
    (*descr).languages = lang.as_ptr();
    (*descr).age = 0;
    (*descr).gender = gender;
    (*descr).identifier = null();
    (*descr).variant = 0;

    descr
}

#[repr(C)]
enum CallbackResponse {
    Continue = 0,
    Abort = 1,
}

extern "C" fn espeak_callback(
    wav: *mut c_short,
    num_samples: c_int,
    _: *mut espeak_EVENT,
) -> c_int {
    // Note: I'm going to leave this poison handling since we are treating directly with C code
    let wav_slc = unsafe {
        std::slice::from_raw_parts(
            wav,
            num_samples
                .try_into()
                .expect("The received number of parts can't fit in a usize, is it negative?"),
        )
    };
    match (*SOUND_BUFFER).lock() {
        Ok(buffer) => {
            buffer.borrow_mut().extend_from_slice(wav_slc);
        }
        Err(poisoned) => {
            warn!("Espeak TTS buffer was corrupted");
            let mut new_buffer = Vec::new();
            new_buffer.extend_from_slice(wav_slc);
            poisoned.into_inner().replace(new_buffer);
        }
    }

    CallbackResponse::Continue as c_int
}

impl EspeakTts {
    pub fn new(lang: &LanguageIdentifier, prefs: &VoiceDescr) -> EspeakTts {
        unsafe {
            espeak_Initialize(
                espeak_AUDIO_OUTPUT::AUDIO_OUTPUT_SYNCHRONOUS,
                0,
                std::ptr::null(),
                0,
            );
        }
        unsafe {
            espeak_SetVoiceByProperties(from_lang_and_prefs(lang, prefs));
        }
        unsafe { espeak_SetSynthCallback(espeak_callback) };

        EspeakTts {}
    }
}

#[async_trait(?Send)]
impl Tts for EspeakTts {
    async fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError> {
        let synth_cstr = CString::new(input.to_string())?;
        let synth_flags = espeakCHARS_AUTO | espeakPHONEMES | espeakENDPAUSE;

        unsafe {
            espeak_Synth(
                synth_cstr.as_ptr() as *const libc::c_void,
                input.len() as usize,
                0,
                espeak_POSITION_TYPE::POS_CHARACTER,
                0,
                synth_flags,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
        }

        match SOUND_BUFFER.lock() {
            Ok(buffer) => Ok(Audio::new_raw(
                buffer.replace(Vec::new()),
                DEFAULT_SAMPLES_PER_SECOND,
            )),
            Err(poisoned) => {
                warn!("Espeak TTS buffer was corrupted");
                poisoned.into_inner().replace(Vec::new());
                Ok(Audio::new_empty(DEFAULT_SAMPLES_PER_SECOND))
            }
        }
    }

    fn get_info(&self) -> TtsInfo {
        //let (info, _data_path) = unsafe {
        let info = unsafe {
            let mut c_buf = std::ptr::null();
            let info = CStr::from_ptr(espeak_Info(&mut c_buf))
                .to_str()
                .unwrap_or("(Info can't be read)");
            //let c_str = CStr::from_ptr(c_buf);
            //let str_slice: &str = c_str.to_str().expect("Can't read espeak data path");
            //let data_path: String = str_slice.to_owned(); // if necessary*

            //(info, data_path)
            info
        };

        TtsInfo {
            name: format!("Espeak TTS {}", info),
            is_online: false,
        }
    }
}

impl Drop for EspeakTts {
    fn drop(&mut self) {
        let err = unsafe { espeak_Terminate() };
        if (err as u8) != (espeak_ERROR::EE_OK as u8) {
            warn!("Error while terminating espeak");
        }
    }
}

impl TtsStatic for EspeakTts {
    type Data = ();
    fn is_descr_compatible(_d: &Self::Data, _descr: &VoiceDescr) -> Result<(), TtsConstructionError> {
        // Espeak is really configurable so it has no problem with what we might
        // want
        Ok(())
    }

    fn is_lang_comptaible(_d: &Self::Data, _lang: &LanguageIdentifier) -> Result<(), TtsConstructionError> {
        // I'm not aware of any language that espeak doesn't implement
        Ok(())
    }
}
