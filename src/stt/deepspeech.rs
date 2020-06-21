use std::mem::replace;
use crate::stt::{SpecifiesLangs, SttConstructionError, SttError, SttBatched, SttInfo, SttVadless};
use crate::vars::{DEEPSPEECH_DATA_PATH, ALPHA_BETA_MSG, SET_BEAM_MSG};
use deepspeech::{CandidateTranscript, Model, Stream};
use unic_langid::{langids, LanguageIdentifier};

// Deepspeech
pub struct DeepSpeechStt {
    model: Model,
    current_stream: Option<Stream>
}

fn transcript_to_string(tr: &CandidateTranscript) -> String {
    let mut res = String::new();
    for token in tr.tokens() {
        res += token.text().unwrap();
    }

    res
}

impl DeepSpeechStt {
    pub fn new(curr_lang: &LanguageIdentifier) -> Result<Self, SttConstructionError> {
        const BEAM_WIDTH:u16 = 500;

        let lang_str = curr_lang.to_string();
        let dir_path = DEEPSPEECH_DATA_PATH.resolve().join(&lang_str);
        if dir_path.is_dir() {
            let mut model = Model::load_from_files(&dir_path.join(&format!("{}.pbmm", &lang_str))).map_err(|_| SttConstructionError::CantLoadFiles)?;
            model.enable_external_scorer(&dir_path.join(&format!("{}.scorer", &lang_str)));
            model.set_scorer_alpha_beta(0.931289039105002f32, 1.1834137581510284f32).expect(ALPHA_BETA_MSG);
            model.set_model_beam_width(BEAM_WIDTH).expect(SET_BEAM_MSG);

            Ok(Self {model, current_stream: None})
        }
        else {
            Err(SttConstructionError::LangIncompatible)
        }
    }
}

impl SttBatched for DeepSpeechStt {
    fn decode(&mut self, audio: &[i16]) -> Result<Option<(String, Option<String>, i32)>, SttError> {
        let metadata = self.model.speech_to_text_with_metadata(audio, 1).unwrap();
        let transcript = &metadata.transcripts()[0];

        Ok(Some((transcript_to_string(transcript), None, transcript.confidence() as i32)))
    }

    fn get_info(&self) -> SttInfo {
        SttInfo {
            name: "DeepSpeech".to_owned(),
            is_online: false
        }
    }
}

impl SttVadless for DeepSpeechStt {
    fn process(&mut self, audio: &[i16]) -> Result<(), SttError> {
        if self.current_stream.is_none() {
            self.current_stream = Some(self.model.create_stream().unwrap())
        }

        match self.current_stream {
            Some(ref mut s) => s.feed_audio(audio),
            None => panic!()
        }

        Ok(())
    }

    fn end_decoding(&mut self) -> Result<Option<(String, Option<String>, i32)>, SttError> {
        let stream = replace(&mut self.current_stream, None).unwrap();
        let metadata = stream.finish_with_metadata(1).unwrap();
        let transcript = &metadata.transcripts()[0];

        Ok(Some((transcript_to_string(transcript), None, transcript.confidence() as i32)))
    }

    fn get_info(&self) -> SttInfo {
        SttInfo {
            name: "DeepSpeech".to_owned(),
            is_online: false
        }
    }
}

impl SpecifiesLangs for DeepSpeechStt {
    fn available_langs() -> Vec<LanguageIdentifier> {
        langids!("en")
    }
}