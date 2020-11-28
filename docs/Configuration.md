# Configuration

You can write a file called `conf.yaml` for configuring your Lily, whether if what
you want is to change some behaviour or to provide some data so it can be used
(like keys for HTTP services like IBM's Voice Synthesis and Speech Recognition 
or Home Assistant) this is the file you are looking for as it host configuration
both for Lily itself as well as for it's plugins.

Description of accepted entries and their use:

Legend:  `name_of_key: type (default)`
- `tts: dict (empty)`: TTS/Voice Syntesis related config
  - `prefer_online: bool (false)`: If `true` Lily will prefer an online service for Voice Synthesis.
  - `ibm: dict (empty)`: IBM's Voice Synsthesis config, if present 
    - `key: string (required)`: The `key` used to send to IBM for it's online Voice Synthesis.
    - `gateway: string (required)`: The `gateway` URL which Lily will connect when using IBM's Voice Synthesis.
- `stt: dict (empty)`: STT/Speech recognition related config
  - `prefer_online: bool (false)`: If `true` Lily will prefer an online service for Speech Recognition.
  - `ibm: dict (empty)`: Data for the ibm STT, can be found in IBM's console
    - `key: string (empty)`: STT's api key
    - `instance: string (empty)`: STT's instance ID ()
    - `gateway: string (empty)`: where is the STT instance located (London, Seoul, ...), not it's URL
- `languages: list of strings (empty)`: A list of languages (in ICU form) that Lily will process and understand, if left empty the current one that the OS uses will be used.Note that the first one will be treated as default in cases that there's no input.
- `hotword_sensitivity: float (0.45)`: The senstivity for the hotword (by default: "Lily") as defined by Snowboy (Bigger value==more easily triggered).
- `debug_record_active_speech: bool (false)`: `true` here makes Lily save an audio file of what was send last time to Speech Recognition (for Speech Recognition debugging purposes).

TTS Note: In order to activate IBM's Voice Synthesis you need to fil `tts/ibm`,
and set `tts/prefer_online` to `true`, however, if cargo feature 
`google_tts` is defined and `tts/ibm`, then Google Tts will be used.

STT Note: In order to have IBM's voice
recognition you need to fill `stt/ibm` and have set 
`stt/prefer_online` to `true`.You can get `gateway`, `ibm_tts_key`/`api_key` and
from IBM after registering an account for their online services,
in their free plan you get a pretty good number of minutes per month.

Also, this file can have map keys with the name of a package, that package will
access that map while getting it's configuration, for example when package `home_assistant`
asks for `auth_key` this is what the config must have:

```yaml
home_assitant:
    auth_key: here_goes_the_actual_key
```

