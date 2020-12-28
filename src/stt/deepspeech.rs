use std::mem::replace;

use crate::stt::{DecodeRes, SpecifiesLangs, SttConstructionError, Stt,SttError, SttBatched, SttInfo};
use crate::vars::{ALPHA_BETA_MSG, DEEPSPEECH_DATA_PATH, DEEPSPEECH_READ_FAIL_MSG, SET_BEAM_MSG};

use async_trait::async_trait;
use anyhow::anyhow;
use deepspeech::{CandidateTranscript, Model, Stream};
use log::warn;
use unic_langid::{LanguageIdentifier};

// Deepspeech
pub struct DeepSpeechStt {
    model: Model,
    current_stream: Option<Stream>
}

fn transcript_to_string(tr: &CandidateTranscript) -> String {
    let mut res = String::new();
    for token in tr.tokens() {
        match token.text() {
            Ok(text) =>  {res += text}
            Err(err) => {warn!("Part of transcript ({}) couldn't be transformed: {:?}", tr, err)}
        }
    }

    res
}

impl DeepSpeechStt {
    pub fn new(curr_lang: &LanguageIdentifier) -> Result<Self, SttConstructionError> {
        const BEAM_WIDTH:u16 = 500;
        const ALPHA: f32 = 0.931289039105002f32;
        const BETA: f32 = 1.1834137581510284f32;

        let lang_str = curr_lang.to_string();
        let dir_path = DEEPSPEECH_DATA_PATH.resolve().join(&lang_str);
        if dir_path.is_dir() {
            let mut model = Model::load_from_files(&dir_path.join(&format!("{}.pbmm", &lang_str))).map_err(|_| SttConstructionError::CantLoadFiles)?;
            model.enable_external_scorer(&dir_path.join(&format!("{}.scorer", &lang_str)))?;
            model.set_scorer_alpha_beta(ALPHA, BETA).expect(ALPHA_BETA_MSG);
            model.set_model_beam_width(BEAM_WIDTH).expect(SET_BEAM_MSG);

            Ok(Self {model, current_stream: None})
        }
        else {
            Err(SttConstructionError::LangIncompatible)
        }
    }
}

#[async_trait(?Send)]
impl SttBatched for DeepSpeechStt {
    async fn decode(&mut self, audio: &[i16]) -> Result<Option<DecodeRes>, SttError> {
        let metadata = self.model.speech_to_text_with_metadata(audio, 1)?;
        let transcript = &metadata.transcripts()[0];

        Ok(Some(DecodeRes{
            hypothesis: transcript_to_string(transcript),
            confidence: transcript.confidence()
        }))
    }

    fn get_info(&self) -> SttInfo {
        SttInfo {
            name: "DeepSpeech".to_owned(),
            is_online: false
        }
    }
}

#[async_trait(?Send)]
impl Stt for DeepSpeechStt {
    async fn begin_decoding(&mut self) -> Result<(), SttError> {
        self.current_stream = Some(self.model.create_stream()?);
        Ok(())
    }
    async fn process(&mut self, audio: &[i16]) -> Result<(), SttError> {
        match self.current_stream {
            Some(ref mut s) => s.feed_audio(audio),
            None => panic!("'process' can't be called before 'begin_decoding'")
        }

        Ok(())
    }

    async fn end_decoding(&mut self) -> Result<Option<DecodeRes>, SttError> {
        let stream = replace(&mut self.current_stream, None).ok_or_else(||panic!("end_decoding can't be called before begin decoding")).unwrap();
        let metadata = stream.finish_with_metadata(1)?;
        let transcript = &metadata.transcripts()[0];

        Ok(Some(DecodeRes{
            hypothesis: transcript_to_string(transcript),
            confidence: transcript.confidence()}
        ))
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

        fn extract_data(entry: Result<std::fs::DirEntry, std::io::Error>) -> Result<Option<LanguageIdentifier>, anyhow::Error> {
            let entry = entry.map_err(|_|anyhow!("Coudln't read an element of deepspeech path due to an intermittent IO error"))?;
            let file_type = entry.file_type().map_err(|_|anyhow!("Couldn't get file type for {:?}", entry.path()))?;
            if file_type.is_dir() {
                let fname = entry.file_name().into_string().map_err(|e|anyhow!("Can't transform to string: {:?}", e))?;
                let lang_id = fname.parse().map_err(|_| anyhow!("Can't parse {} as language identifier", fname))?;
                Ok(Some(lang_id))
            }
            else {
                Ok(None)
            }
        }

        fn extract(entry: Result<std::fs::DirEntry, std::io::Error>) -> Option<LanguageIdentifier> {
            match extract_data(entry) {
                Ok(data) => {
                    data
                },
                Err(e) => {
                    warn!("{}", e);
                    None
                }
            }
        }

        match DEEPSPEECH_DATA_PATH.resolve().read_dir() {
            Ok(dir_it) => {
                dir_it.filter_map(extract).collect()
            },
            Err(e) => {
                warn!("{}:{}", DEEPSPEECH_READ_FAIL_MSG, e);
                vec![]
            }

        }
    }
}