use std::mem::replace;
use crate::stt::{SttConstructionError, SttError, SttBatched, SttInfo, SttVadless};
use crate::vars::DEEPSPEECH_DATA_PATH;
use deepspeech::{CandidateTranscript, Model, Stream};

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
    pub fn new() -> Result<Self, SttConstructionError> { 
        //const BEAM_WIDTH:u16 = 500;
        //const LM_WEIGHT:f32 = 16_000f32;

        let dir_path = DEEPSPEECH_DATA_PATH.resolve();
        let model = Model::load_from_files(&dir_path.join("output_graph.pb")).map_err(|_| SttConstructionError::CantLoadFiles)?;
        //model.enable_decoder_with_lm(&dir_path.join("lm.binary"),&dir_path.join("trie"), LM_WEIGHT, VALID_WORD_COUNT_WEIGHT);

        Ok(Self {model, current_stream: None})
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