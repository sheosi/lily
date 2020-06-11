use crate::stt::{SttConstructionError, SttError, SttBatched, SttInfo};
use deepspeech::Model;
use crate::vars::DEEPSPEECH_DATA_PATH;

// Deepspeech
pub struct DeepSpeechStt {
    model: deepspeech::Model
}

impl DeepSpeechStt {
    pub fn new() -> Result<Self, SttConstructionError> { 
        //const BEAM_WIDTH:u16 = 500;
        //const LM_WEIGHT:f32 = 16_000f32;

        let dir_path = DEEPSPEECH_DATA_PATH.resolve();
        let model = Model::load_from_files(&dir_path.join("output_graph.pb")).map_err(|_| SttConstructionError::CantLoadFiles)?;
        //model.enable_decoder_with_lm(&dir_path.join("lm.binary"),&dir_path.join("trie"), LM_WEIGHT, VALID_WORD_COUNT_WEIGHT);

        Ok(Self {model})
    }
}

impl SttBatched for DeepSpeechStt {
    fn decode(&mut self, audio: &[i16]) -> Result<Option<(String, Option<String>, i32)>, SttError> {
        Ok(Some((self.model.speech_to_text(audio).unwrap(), None, 0)))
    }

    fn get_info(&self) -> SttInfo {
        SttInfo {
            name: "DeepSpeech".to_owned(),
            is_online: false
        }
    }
}