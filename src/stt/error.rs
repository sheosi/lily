use thiserror::Error;
use crate::path_ext::NotUnicodeError;

#[derive(Error, Debug)]
pub enum SttError {
    #[error("PocketSphinx error, see log for details")]
    PocketsphinxError(#[from] pocketsphinx::Error),

    #[error("Error while sending to online service")]
    OnlineError(#[from] OnlineSttError),

    #[error("Error from the vad")]
    Vad(#[from] crate::vad::VadError)
}

#[derive(Error, Debug)]
pub enum SttConstructionError {
    #[error("The input language identifier has no region")]
    NoRegion,

    #[error("PocketSphinx error, see log for details")]
    PocketsphinxError(#[from] pocketsphinx::Error),

    #[error("Input path was not unicode")]
    NotUnicodeError(#[from] NotUnicodeError),

    #[error("Input is not a valid utf-8")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[cfg(feature = "devel_deepspeech")]
    #[error("Language not compatible")]
    LangIncompatible,

    #[cfg(feature = "devel_deepspeech")]
    #[error("Can't load files")]
    CantLoadFiles,
}

#[derive(Error,Debug)]
pub enum OnlineSttError {
	#[error("network failure")]
	Network(#[from] reqwest::Error),

	#[error("url parsing")]
	UrlParse(#[from] url::ParseError),

	#[error("wav conversion")]
	WavConvert(#[from] crate::audio::AudioError),

	#[error("json parsing")]
	JsonParse(#[from] serde_json::Error),

	#[error("opus encoding")]
	OpusEncode(#[from] opus::Error)
}
