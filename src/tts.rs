use core::fmt::Display;

use crate::vars::{PICO_DATA_PATH};
use crate::audio::Audio;
use crate::path_ext::{NotUnicodeError,ToStrResult};

use thiserror::Error;
use unic_langid::{LanguageIdentifier, langid, langids};
use fluent_langneg::{negotiate_languages, NegotiationStrategy};

#[derive(Error, Debug, Clone)]
pub enum TtsError {
    #[error("Input string had a nul character")]
    StringHadInternalNul(#[from] std::ffi::NulError)
}

#[derive(Debug, Clone)]
pub struct TtsInfo {
    pub name: String,
    pub is_online: bool
}

#[derive(Error, Debug, Clone)]
pub enum TtsConstructionError {
        #[error("No voice with the selected gender is available")]
        WrongGender,

        #[error("This engine is not available in this language")]
        IncompatibleLanguage,

        #[error("Input language has no region")]
        NoRegion,

        #[error("Input is not unicode")]
        NotUnicode(#[from] NotUnicodeError)
}

impl Display for TtsInfo {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        let online_str = {
            if self.is_online {"online"}
            else {"local"}

        };
        
        write!(formatter, "{}({})", self.name, online_str)
    }
}


#[derive(Debug, Clone, PartialEq)]
pub enum Gender {
    Male,
    Female
}
#[derive(Debug, Clone)]
pub struct VoiceDescr {
    pub gender: Gender
}

pub trait Tts {
    fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError>;
    fn get_info(&self) -> TtsInfo;
}

pub trait TtsStatic {
    fn check_compatible(descr: &VoiceDescr) -> Result<(), TtsConstructionError>;
}

#[cfg(feature = "extra_langs_tts")]
pub struct EspeakTts {    
}

#[cfg(feature = "extra_langs_tts")]
impl EspeakTts {
    pub fn new(_lang: &LanguageIdentifier)  -> EspeakTts {
        unsafe {espeak_sys::espeak_Initialize(espeak_sys::espeak_AUDIO_OUTPUT::AUDIO_OUTPUT_PLAYBACK, 0, std::ptr::null(), 0);}
        //espeak_SetSynthCallback();

        EspeakTts{}
    }
}

#[cfg(feature = "extra_langs_tts")]
impl Tts for EspeakTts {
    fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError> {
        let synth_cstr = std::ffi::CString::new(input.to_string())?;
        let synth_flags = espeak_sys::espeakCHARS_AUTO | espeak_sys::espeakPHONEMES | espeak_sys::espeakENDPAUSE;

        // input.len().try_into().unwrap() -> size_t is the same as usize
        unsafe {espeak_sys::espeak_Synth(synth_cstr.as_ptr() as *const std::ffi::c_void , input.len() as libc::size_t, 0, espeak_sys::espeak_POSITION_TYPE::POS_CHARACTER, 0, synth_flags, std::ptr::null_mut(), std::ptr::null_mut());}

        Ok(Audio {buffer:vec![], samples_per_second: 16000})
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo {
            name: "Espeak TTS",
            is_online: false
        }   
    }
}

#[cfg(feature = "extra_langs_tts")]
impl TtsStatic for EspeakTts {
    fn check_compatible(_descr: &VoiceDescr) -> bool {
        // Espeak is really onfigurable so it has no problem with what we might
        // want
        Ok(())
    }

}

// The MIT License
//
// Copyright (c) 2019 Paolo Jovon <paolo.jovon@gmail.com>
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

use ttspico as pico;

pub struct PicoTts {
    engine: pico::Engine
}


impl PicoTts {

    // Just accept a negotiated LanguageIdentifier
    fn sg_name(lang: &LanguageIdentifier) -> Result<&'static str, TtsConstructionError> {
        let lang_str = format!("{}-{}", lang.language, lang.region.ok_or(TtsConstructionError::NoRegion)?);
        match lang_str.as_str()  {
            "es-ES" => Ok("es-ES_zl0_sg.bin"),
            "en-US" => Ok("en-US_lh0_sg.bin"),
            _ => Err(TtsConstructionError::IncompatibleLanguage)
        }

    }

    fn ta_name(lang: &LanguageIdentifier) -> Result<String, TtsConstructionError> {
        Ok(format!("{}-{}_ta.bin", lang.language , lang.region.ok_or(TtsConstructionError::NoRegion)?.as_str()))
    }

    // There's only one voice of Pico per language so preferences are not of much use here
    pub fn new(lang: &LanguageIdentifier, prefs: &VoiceDescr) -> Result<Self, TtsConstructionError> {
        Self::check_compatible(prefs)?; // Check voice description compatibility

        // 1. Create a Pico system
        let lang = Self::lang_neg(lang)?;
        let sys = pico::System::new(4 * 1024 * 1024).expect("Could not init system");
        let lang_path = PICO_DATA_PATH.resolve();

        // 2. Load Text Analysis (TA) and Speech Generation (SG) resources for the voice you want to use
        let ta_res = 
            pico::System::load_resource(sys.clone(), lang_path.join(Self::ta_name(&lang)?).to_str_res()?)
            .expect("Failed to load TA");
        let sg_res = pico::System::load_resource(sys.clone(), lang_path.join(Self::sg_name(&lang)?).to_str_res()?)
            .expect("Failed to load SG");


        // 3. Create a Pico voice definition and attach the loaded resources to it
        let voice = pico::System::create_voice(sys.clone(), "TestVoice")
        .expect("Failed to create voice");
    voice
        .borrow_mut().add_resource(ta_res)
        .expect("Failed to add TA to voice");
    voice
        .borrow_mut().add_resource(sg_res)
        .expect("Failed to add SG to voice");


        // 4. Create an engine from the voice definition
        // UNSAFE: Creating an engine without attaching the resources will result in a crash!
        let engine = unsafe { pico::Voice::create_engine(voice.clone()).expect("Failed to create engine") };
        //let voice_def = espeak_sys::espeak_VOICE{name: std::ptr::null(), languages: std::ptr::null(), identifier: std::ptr::null(), gender: 0, age: 0, variant: 0, xx1:0, score: 0, spare: std::ptr::null_mut()};
        Ok(PicoTts{engine})
    }

    fn lang_neg(lang: &LanguageIdentifier) -> Result<LanguageIdentifier, TtsConstructionError> {
        let available_langs = langids!("es-ES", "en-US");
        let default = langid!("en-US");

        let langs = negotiate_languages(&[lang],&available_langs, Some(&default), NegotiationStrategy::Filtering);
        if !langs.is_empty() {
            Ok(langs[0].clone())
        }
        else {
            Err(TtsConstructionError::IncompatibleLanguage)
        }
    }
}

impl Tts for PicoTts {
    fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError> {
        // 5. Put (UTF-8) text to be spoken into the engine
        // See `Engine::put_text()` for more details.
        let input = std::ffi::CString::new(input).expect("CString::new failed");
        let mut text_bytes = input.as_bytes_with_nul();
        while text_bytes.len() > 0 {
            let n_put = self.engine
                .put_text(text_bytes)
                .expect("pico_putTextUtf8 failed");
            text_bytes = &text_bytes[n_put..];
        }

        // 6. Do the actual text-to-speech, getting audio data (16-bit signed PCM @ 16kHz) from the input text
        // Speech audio is computed in small chunks, one "step" at a time; see `Engine::get_data()` for more details.
        let mut pcm_data = vec![0i16; 0];
        let mut pcm_buf = [0i16; 1024];
        'tts: loop {
            let (n_written, status) = self.engine
                .get_data(&mut pcm_buf[..])
                .expect("pico_getData error");
            pcm_data.extend(&pcm_buf[..n_written]);
            if status == ttspico::EngineStatus::Idle {
                break 'tts;
            }
        }

        Ok(Audio::new_raw(pcm_data, 16000))
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo {
            name: "Pico Tts".to_string(),
            is_online: false
        }
    }
}

impl TtsStatic for PicoTts {
    fn check_compatible(descr: &VoiceDescr) -> Result<(), TtsConstructionError> {
        //Only has female voices (by default)
        if descr.gender != Gender::Female {
            Err(TtsConstructionError::WrongGender)
        }
        else {
            Ok(())
        }
    }
}

#[cfg(feature = "google_tts")]
struct GTts {
    engine: crate::gtts::GttsEngine,
    fallback_tts : Box<dyn Tts>,
    curr_lang: String
}

#[cfg(feature = "google_tts")]
impl GTts {

    pub fn new(lang: &LanguageIdentifier, fallback_tts: Box<dyn Tts>) -> Self {
        GTts{engine: crate::gtts::GttsEngine::new(), fallback_tts, curr_lang: Self::make_tts_lang(&Self::lang_neg(lang)).to_string()}
    }

    fn make_tts_lang<'a>(lang: &'a LanguageIdentifier) -> &'a str {
        lang.get_language()
    }

    fn lang_neg(lang: &LanguageIdentifier) -> LanguageIdentifier {
        let available_langs = langids!("es", "en");
        let default = langid!("en");
        negotiate_languages(&[lang],&available_langs, Some(&default), NegotiationStrategy::Filtering)[0].clone()
    }
}

#[cfg(feature = "google_tts")]
impl Tts for GTts {
    fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError> {
        match self.engine.synth(input, &self.curr_lang) {
            Ok(buffer) => {Ok(Audio::new_encoded(buffer, 16000))},
            Err(_) => {
                // If it didn't work try with local
                self.fallback_tts.synth_text(input)
            }
        }
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo {
            name: "Google Translate".to_string(),
            is_online: true
        }
    }
}

#[cfg(feature = "google_tts")]
impl TtsStatic for GTts {
    fn is_compatible(descr: &VoiceDescr) -> bool {
        true
    }
}

struct IbmTts {
    engine: crate::gtts::IbmTtsEngine,
    fallback_tts : Box<dyn Tts>,
    curr_voice: String
}

impl IbmTts {
    pub fn new(lang: &LanguageIdentifier, fallback_tts: Box<dyn Tts>, api_gateway: String, api_key: String, prefs: &VoiceDescr) -> Result<Self, TtsConstructionError> {
        Ok(IbmTts{engine: crate::gtts::IbmTtsEngine::new(api_gateway, api_key), fallback_tts, curr_voice: Self::make_tts_voice(&Self::lang_neg(lang), prefs)?.to_string()})
    }

    // Accept only negotiated LanguageIdentifiers
    fn make_tts_voice(lang: &LanguageIdentifier, prefs: &VoiceDescr) -> Result<&'static str, TtsConstructionError> {
        let lang_str = format!("{}-{}", lang.language, lang.region.ok_or(TtsConstructionError::NoRegion)?.as_str());
        match lang_str.as_str() {
            "es-ES" => {
                Ok(match prefs.gender {
                    Gender::Male => "es-ES_EnriqueV3Voice",
                    Gender::Female => "es-ES_LauraV3Voice"
                })
            }
            "en-US" => {
                Ok(match prefs.gender {
                    Gender::Male => "en-US_MichaelV3Voice",
                    Gender::Female =>  "en-US_AllisonV3Voice"
                })
            }
            _ => Err(TtsConstructionError::IncompatibleLanguage)
        }
    }

    // Accept only negotiated LanguageIdentifiers
    fn lang_neg(lang: &LanguageIdentifier) -> LanguageIdentifier {
        let available_langs = langids!("es-ES", "en-US");
        let default = langid!("en-US");
        negotiate_languages(&[&lang],&available_langs, Some(&default), NegotiationStrategy::Filtering)[0].clone()
    }
}

impl Tts for IbmTts {
    fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError> {
        match self.engine.synth(input, &self.curr_voice) {
            Ok(buffer) => {Ok(Audio::new_encoded(buffer, 16000))},
            Err(_) => {
                // If it didn't work try with local
                self.fallback_tts.synth_text(input)
            }
        }
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo {
            name: "IBM Text To Speech".to_string(),
            is_online: true
        }
    }
}


impl TtsStatic for IbmTts {
    fn check_compatible(_descr: &VoiceDescr) -> Result<(),TtsConstructionError> {
        // Ibm has voices for both genders in all supported languages
        Ok(())
    }
}

pub struct DummyTts{}

impl DummyTts {
    pub fn new() -> Self{
        Self{}
    }
}

impl Tts for DummyTts {
    fn synth_text(&mut self, _input: &str) -> Result<Audio, TtsError> {
        Ok(Audio::new_empty(16000))
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo{
            name: "Dummy Synthesizer".to_string(),
            is_online: false
        }
    }
}

impl TtsStatic for DummyTts {
    fn check_compatible(_descr: &VoiceDescr) -> Result<(), TtsConstructionError> {
        // Just a dummy, won't output anything anyway
        Ok(())
    }
}

pub struct TtsFactory;

impl TtsFactory {
    pub fn load_with_prefs(lang: &LanguageIdentifier, prefer_cloud_tts: bool, gateway_key: Option<(String, String)>, prefs: &VoiceDescr) -> Result<Box<dyn Tts>, TtsConstructionError> {
        let local_tts = Box::new(PicoTts::new(lang, prefs)?);

        match prefer_cloud_tts {
            true => {
                if let Some((api_gateway, api_key)) = gateway_key {
                    Ok(Box::new(IbmTts::new(lang, local_tts, api_gateway.to_string(), api_key.to_string(), prefs)?))
                }
                else {
                    Ok(local_tts)
                }
            },
            false => {
                Ok(local_tts)
            }
        }
    }

    pub fn dummy() -> Box<dyn Tts> {
        Box::new(DummyTts::new())
    }
}