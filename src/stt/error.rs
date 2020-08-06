use thiserror::Error;
use crate::path_ext::NotUnicodeError;

#[derive(Error, Debug)]
pub enum SttError {
    #[error("PocketSphinx error, see log for details")]
    PocketsphinxError(#[from] pocketsphinx::Error),

    #[error("Error while sending to online service")]
    OnlineError(#[from] OnlineSttError),

    #[error("Error from the vad")]
    Vad(#[from] crate::vad::VadError),

    #[cfg(feature = "deepspeech_stt")]
    #[error("Deepspeech error")]
    Deepspeech(#[from] deepspeech::errors::DeepspeechError)
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

    #[error("Language not compatible")]
    LangIncompatible,

    #[cfg(feature = "deepspeech_stt")]
    #[error("Can't load files")]
    CantLoadFiles,

    #[error("Vad couldn't be constructed")]
    CantConstrucVad(#[from] crate::vad::VadError)
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
    OpusEncode(#[from] opus::Error),

    #[error("Websocket failure")]
	WebSocket(#[from] tungstenite::Error)
}
