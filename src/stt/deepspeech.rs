// Deepspeech
#[cfg(feature = "devel_deepspeech")]
pub struct DeepSpeechStt {
    model: deepspeech::Model
}

#[cfg(feature = "devel_deepspeech")]
impl DeepspeechStt { 
    pub fn new() -> Result<Self, SttConstructionError> { 
        const BEAM_WIDTH:u16 = 500;
        const LM_WEIGHT:f32 = 16_000;

        let mut model = deepspeech::Model::load_from_files(&dir_path.join("output_graph.pb"), BEAM_WIDTH).map_err(|_| SttError::CantLoadFiles)?;
        model.enable_decoder_with_lm(&dir_path.join("lm.binary"),&dir_path.join("trie"), LM_WEIGHT, VALID_WORD_COUNT_WEIGHT);

        Self {model}
    }
}

#[cfg(feature = "devel_deepspeech")]
impl SttBatched for DeepspeechStt {
    fn decode(&mut self, audio: &[i16]) -> Result<DecodeState, SttError> {
        Ok(DecodeState::Finished(m.speech_to_text(&audio_buf)?))
    }

    fn get_info() -> SttInfo {
        SttInfo {name: "DeepSpeech", is_online: false}
    }
}