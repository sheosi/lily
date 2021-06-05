use thiserror::Error;
use crate::exts::NotUnicodeError;

#[derive(Error, Debug)]
pub enum TtsError {
    #[error("Input string had a nul character")]
    StringHadInternalNul(#[from] std::ffi::NulError),

    #[error("Problem with online TTS")]
    Online(#[from] OnlineTtsError)
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

#[derive(Error,Debug)]
pub enum OnlineTtsError {
	#[error("network failure")]
	Network(#[from] reqwest::Error),

	#[error("url parsing")]
    UrlParse(#[from] url::ParseError),
}
