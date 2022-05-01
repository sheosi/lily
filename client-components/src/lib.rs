use std::path::Path;

use anyhow::Result;
use lily_common::{
    audio::AudioRaw,
    communication::ClientConf,
    client::{
        hotword::HotwordDetector,
        vad::{Vad, VadError}
    },
    vars::DEFAULT_SAMPLES_PER_SECOND
};

pub struct ActiveListener<V: Vad> {
    pub was_talking: bool,
    vad: V,
}

pub struct PasiveListener<H: HotwordDetector> {
    hotword_detector: H,
}

impl<H: HotwordDetector> PasiveListener<H> {
    pub fn new(mut hotword_detector: H) -> Result<Self> {
        hotword_detector.start_hotword_check()?;
        Ok(Self { hotword_detector })
    }

    pub fn process(&mut self, audio: AudioRef) -> Result<bool> {
        self.hotword_detector.check_hotword(audio.data)
    }

    pub fn set_from_conf(&mut self, conf: &ClientConf) {
        self.hotword_detector
            .set_sensitivity(conf.hotword_sensitivity)
    }
}

pub enum ActiveState<'a> {
    // TODO: Add timeout
    NoOneTalking,
    Hearing(AudioRef<'a>),
    Done(AudioRef<'a>),
}

impl<V: Vad> ActiveListener<V> {
    pub fn new(vad: V) -> Self {
        Self {
            was_talking: false,
            vad,
        }
    }

    pub fn process<'a>(&mut self, audio: AudioRef<'a>) -> Result<ActiveState<'a>, VadError> {
        if self.vad.is_someone_talking(audio.data)? {
            self.was_talking = true;
            Ok(ActiveState::Hearing(audio))
        } else if self.was_talking {
            self.vad.reset()?;
            self.was_talking = false;
            Ok(ActiveState::Done(audio))
        } else {
            Ok(ActiveState::NoOneTalking)
        }
    }
}

#[derive(Clone)]
pub struct AudioRef<'a> {
    pub data: &'a [i16],
}

impl<'a> AudioRef<'a> {
    pub fn from(data: &'a [i16]) -> Self {
        Self { data }
    }

    pub fn into_owned(self) -> AudioRaw {
        AudioRaw::new_raw(self.data.to_owned(), DEFAULT_SAMPLES_PER_SECOND)
    }
}

#[cfg(debug_assertions)]
pub struct DebugAudio {
    audio: AudioRaw,
    save_ms: u16,
    curr_ms: f32,
}

#[cfg(debug_assertions)]
impl DebugAudio {
    pub fn new(save_ms: u16) -> Self {
        Self {
            audio: AudioRaw::new_empty(DEFAULT_SAMPLES_PER_SECOND),
            save_ms,
            curr_ms: 0.0,
        }
    }

    pub fn push(&mut self, audio: &AudioRef) {
        self.curr_ms += (audio.data.len() as f32) / (DEFAULT_SAMPLES_PER_SECOND as f32) * 1000.0;
        self.audio
            .append_audio(audio.data, DEFAULT_SAMPLES_PER_SECOND)
            .expect("Wrong SPSs");
        if (self.curr_ms as u16) >= self.save_ms {
            self.audio
                .save_to_disk(Path::new("pasive_audio.ogg"))
                .expect("Failed to write debug file");
            self.clear();
        }
    }

    pub fn clear(&mut self) {
        self.audio.clear();
        self.curr_ms = 0.0;
    }
}

// Just an empty version of DebugAudio for release
#[cfg(not(debug_assertions))]
pub struct DebugAudio {
}

#[cfg(not(debug_assertions))]
impl DebugAudio {
    pub fn new(save_ms: u16) -> Self {Self {}}

    pub fn push(&mut self, audio: &AudioRef) {}

    pub fn clear(&mut self) {}
}
