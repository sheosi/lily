//! Implements espeak TTS, note because of how espeak works there can only be
//! one instance of EspeakTTS in the whole program
use core::convert::TryInto;
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::ptr::null;
use std::sync::Mutex;

use crate::audio::Audio;
use crate::tts::{Gender, TtsError, TtsConstructionError,  VoiceDescr, TtsInfo, Tts, TtsStatic};
use crate::vars::DEFAULT_SAMPLES_PER_SECOND;

use espeak_ng_sys::*;
use lazy_static::lazy_static;
use libc::{c_short, c_int};
use log::warn;
use unic_langid::LanguageIdentifier;

lazy_static! {
    static ref SOUND_BUFFER: Mutex<RefCell<Vec<i16>>> = Mutex::new(RefCell::new(Vec::new()));
}

pub struct EspeakTts {
}

unsafe fn  from_lang_and_prefs(lang: &LanguageIdentifier, prefs: &VoiceDescr) -> *mut espeak_VOICE {
    let lang = CString::new(lang.language.as_str()).unwrap();
    let gender = match prefs.gender {
        Gender::Male => 1,
        Gender::Female => 2
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
    Abort = 1
}

extern "C" fn espeak_callback(wav: *mut c_short, num_samples: c_int, _: *mut espeak_EVENT) -> c_int {
    let wav_slc = unsafe {std::slice::from_raw_parts(wav, num_samples.try_into().unwrap())};
    (*SOUND_BUFFER).lock().unwrap().borrow_mut().extend_from_slice(wav_slc);

    CallbackResponse::Continue as c_int
}


impl EspeakTts {
    pub fn new(lang: &LanguageIdentifier, prefs: &VoiceDescr)  -> EspeakTts {
        unsafe {espeak_Initialize(espeak_AUDIO_OUTPUT::AUDIO_OUTPUT_SYNCHRONOUS, 0, std::ptr::null(), 0);}
        unsafe {espeak_SetVoiceByProperties(from_lang_and_prefs(lang, prefs));}
        unsafe {espeak_SetSynthCallback(espeak_callback)};

        EspeakTts{}
    }
}


impl Tts for EspeakTts {
    fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError> {
        let synth_cstr = CString::new(input.to_string())?;
        let synth_flags = espeakCHARS_AUTO | espeakPHONEMES | espeakENDPAUSE;

        // input.len().try_into().unwrap() -> size_t is the same as usize
        unsafe {espeak_Synth(synth_cstr.as_ptr() as *const libc::c_void , input.len() as usize, 0, espeak_POSITION_TYPE::POS_CHARACTER, 0, synth_flags, std::ptr::null_mut(), std::ptr::null_mut());}


        Ok(Audio::new_raw(SOUND_BUFFER.lock().unwrap().replace(Vec::new()), DEFAULT_SAMPLES_PER_SECOND))
    }

    fn get_info(&self) -> TtsInfo {
        let (info, _data_path) = unsafe {
            let mut c_buf = std::ptr::null();
            let info = CStr::from_ptr(espeak_Info(&mut c_buf)).to_str().unwrap();
            let c_str = CStr::from_ptr(c_buf);
            let str_slice: &str = c_str.to_str().unwrap();
            let data_path: String = str_slice.to_owned(); // if necessary*

            (info, data_path)
        };


        TtsInfo {
            name: format!("Espeak TTS {}", info),
            is_online: false
        }
    }
}

impl Drop for EspeakTts {
    fn drop(&mut self) {
        let err = unsafe{espeak_Terminate()};
        if (err as u8) != (espeak_ERROR::EE_OK as u8) {
            warn!("Error while terminating espeak");
        }
    }
}

impl TtsStatic for EspeakTts {
    fn is_descr_compatible(_descr: &VoiceDescr) -> Result<(), TtsConstructionError> {
        // Espeak is really onfigurable so it has no problem with what we might
        // want
        Ok(())
    }

    fn is_lang_comptaible(_lang: &LanguageIdentifier) -> Result<(), TtsConstructionError> {
        // I'm not aware of any language that espeak doesn't implement
        Ok(())
    }

}