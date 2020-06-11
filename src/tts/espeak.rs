use std::ffi::CString;
use crate::audio::Audio;
use crate::tts::{TtsError, TtsConstructionError,  VoiceDescr, TtsInfo, Tts, TtsStatic};

use unic_langid::LanguageIdentifier;
use espeak_ng_sys::*;
use crate::vars::DEFAULT_SAMPLES_PER_SECOND;

pub struct EspeakTts {
}

impl EspeakTts {
    pub fn new(_lang: &LanguageIdentifier)  -> EspeakTts {
        unsafe {espeak_Initialize(espeak_AUDIO_OUTPUT::AUDIO_OUTPUT_PLAYBACK, 0, std::ptr::null(), 0);}
        //espeak_SetSynthCallback();

        EspeakTts{}
    }
}


impl Tts for EspeakTts {
    fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError> {
        let synth_cstr = CString::new(input.to_string())?;
        let synth_flags = espeakCHARS_AUTO | espeakPHONEMES | espeakENDPAUSE;

        // input.len().try_into().unwrap() -> size_t is the same as usize
        unsafe {espeak_Synth(synth_cstr.as_ptr() as *const libc::c_void , input.len() as usize, 0, espeak_POSITION_TYPE::POS_CHARACTER, 0, synth_flags, std::ptr::null_mut(), std::ptr::null_mut());}

        Ok(Audio::new_empty(DEFAULT_SAMPLES_PER_SECOND))
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo {
            name: "Espeak TTS".to_owned(),
            is_online: false
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