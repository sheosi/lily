use thiserror::Error;
use lily_common::audio::AudioError;
use crate::path_ext::NotUnicodeError;

#[derive(Error, Debug)]
pub enum SttError {
    #[error("PocketSphinx error, see log for details")]
    PocketsphinxError(#[from] pocketsphinx::Error),

    #[error("Error while sending to online service: {0}")]
    OnlineError(#[from] OnlineSttError),

    #[cfg(feature = "deepspeech_stt")]
    #[error("Deepspeech error")]
    Deepspeech(#[from] deepspeech::errors::DeepspeechError),

    #[error("Failed to append audio")]
    AudioError(#[from] AudioError)
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

    #[cfg(feature = "deepspeech_stt")]
    #[error("Deepspeech error")]
    Deepspeech(#[from] deepspeech::errors::DeepspeechError)
}

#[derive(Error,Debug)]
pub enum OnlineSttError {
	#[error("network failure")]
	Network(#[from] reqwest::Error),

	#[error("url parsing")]
	UrlParse(#[from] url::ParseError),

	#[error("wav conversion")]
	WavConvert(#[from] lily_common::audio::AudioError),

	#[error("json parsing")]
	JsonParse(#[from] serde_json::Error),

	#[error("opus encoding")]
    OpusEncode(#[from] magnum_opus::Error),

    #[error("Websocket failure: {0}")]
    WebSocket(#[from] tungstenite::Error),
    
    #[error("Connection closed")]
    ConnectionClosed
}
