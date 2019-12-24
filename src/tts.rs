use core::fmt::Display;
use core::cell::RefCell;
use std::rc::Rc;

use crate::vars::{PICO_DATA_PATH, resolve_path};
use crate::audio::Audio;

use unic_langid::{LanguageIdentifier, langid, langids};
use fluent_langneg::{negotiate_languages, NegotiationStrategy};

#[derive(Debug, Clone)]
pub enum TtsErrCause {
    StringHadInternalNul
}

#[derive(Debug, Clone)]
pub struct TtsError {
    cause: TtsErrCause
}

#[derive(Debug, Clone)]
pub struct TtsInfo {
    pub name: String,
    pub is_online: bool
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

impl std::convert::From<std::ffi::NulError> for TtsError {
    fn from(_nul_err: std::ffi::NulError) -> Self {
        TtsError{cause: TtsErrCause::StringHadInternalNul}
    }
}

impl std::fmt::Display for TtsError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.cause {
            TtsErrCause::StringHadInternalNul => {
                write!(f, "The string you asked for had internal nulls ('\0') which is incompatible with C")
            }
        }
    }
}

pub trait Tts {
    fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError>;
    fn get_info(&self) -> TtsInfo;
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
    sys: Rc<RefCell<pico::System>>,
    voice: Rc<RefCell<pico::Voice>>,
    engine: pico::Engine
}


impl PicoTts {

    // Just accept a negotiated LanguageIdentifier
    fn sg_name(lang: &LanguageIdentifier) -> &'static str {
        let lang_str = format!("{}-{}", lang.get_language(), lang.get_region().unwrap());
        match lang_str.as_str()  {
            "es-ES" => "es-ES_zl0_sg.bin",
            "en-US" => "en-US_lh0_sg.bin",
            _ => ""
        }

    }

    fn ta_name(lang: &LanguageIdentifier) -> String {
        format!("{}-{}_ta.bin", lang.get_language() , lang.get_region().unwrap())
    }

    pub fn new(lang: &LanguageIdentifier) -> Self {
        // 1. Create a Pico system
        let lang = Self::lang_neg(lang);
        let sys = pico::System::new(4 * 1024 * 1024).expect("Could not init system");
        let lang_path = resolve_path(PICO_DATA_PATH);

        // 2. Load Text Analysis (TA) and Speech Generation (SG) resources for the voice you want to use
        let ta_res = 
            pico::System::load_resource(sys.clone(), lang_path.join(Self::ta_name(&lang)).to_str().unwrap())
            .expect("Failed to load TA");
        let sg_res = pico::System::load_resource(sys.clone(), lang_path.join(Self::sg_name(&lang)).to_str().unwrap())
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
        PicoTts{sys, voice, engine}
    }

    fn lang_neg(lang: &LanguageIdentifier) -> LanguageIdentifier {
        let available_langs = langids!("es-ES", "en-US");
        let default = langid!("en-US");
        negotiate_languages(&[lang],&available_langs, Some(&default), NegotiationStrategy::Filtering)[0].clone()
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

        Ok(Audio{buffer: pcm_data, samples_per_second: 16000})
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo {
            name: "Pico Tts".to_string(),
            is_online: false
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
            Ok(_) => {Ok(Audio{buffer:Vec::new(), samples_per_second: 16000})},
            Err(_) => {
                // If it didn't work try with local
                self.fallback_tts.synth_text(input)
            }
        }
    }

    fn get_info() -> TtsInfo {
        TtsInfo {
            name: "Google Translate",
            is_online: true
        }
    }
}

struct IbmTts {
    engine: crate::gtts::IbmTtsEngine,
    fallback_tts : Box<dyn Tts>,
    curr_voice: String
}

impl IbmTts {
    pub fn new(lang: &LanguageIdentifier, fallback_tts: Box<dyn Tts>, api_gateway: String, api_key: String) -> Self {
        IbmTts{engine: crate::gtts::IbmTtsEngine::new(api_gateway, api_key), fallback_tts, curr_voice: Self::make_tts_voice(&Self::lang_neg(lang)).to_string()}
    }

    // Accept only negotiated LanguageIdentifiers
    fn make_tts_voice(lang: &LanguageIdentifier) -> &'static str {
        let lang_str = format!("{}-{}", lang.get_language(), lang.get_region().unwrap());
        match lang_str.as_str() {
            "es-ES" => "es-ES_LauraV3Voice",
            "en-US" => "en-US_AllisonV3Voice",
            _ => ""
        }
    }

    // Accept only negotiated LanguageIdentifiers
    fn lang_neg(lang: &LanguageIdentifier) -> LanguageIdentifier {
        let available_langs = langids!("es-ES", "en-US");
        let default = langid!("en-US");
        negotiate_languages(&[lang],&available_langs, Some(&default), NegotiationStrategy::Filtering)[0].clone()
    }
}

impl Tts for IbmTts {
    fn synth_text(&mut self, input: &str) -> Result<Audio, TtsError> {
        match self.engine.synth(input, &self.curr_voice) {
            Ok(_) => {Ok(Audio{buffer:Vec::new(), samples_per_second: 16000})},
            Err(_) => {
                // If it didn't work try with local
                self.fallback_tts.synth_text(input)
            }
        }
    }

    fn get_info(&self) -> TtsInfo {
        TtsInfo {
            name: "IBM Speech To Text".to_string(),
            is_online: true
        }
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

pub struct TtsFactory;

impl TtsFactory {
    pub fn load(lang: &LanguageIdentifier, prefer_cloud_tss: bool, gateway_key: Option<(String, String)>) -> Box<dyn Tts> {
        
        let local_tts = Box::new(PicoTts::new(lang));

        match prefer_cloud_tss {
            true => {
                if let Some((api_gateway, api_key)) = gateway_key {
                    Box::new(IbmTts::new(lang, local_tts, api_gateway.to_string(), api_key.to_string()))
                }
                else {
                    local_tts
                }
            },
            false => {local_tts}
        }
    }

    pub fn dummy() -> Box<dyn Tts> {
        Box::new(DummyTts::new())
    }
}