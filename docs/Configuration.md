# Configuration

You can write a file called `conf.yaml` for configuring your Lily, whether if what
you want is to change some behaviour or to provide some data so it can be used
(like keys for HTTP services like IBM's Voice Synthesis and Speech Recognition 
or Home Assistant) this is the file you are looking for as it host configuration
both for Lily itself as well as for it's plugins.

Description of accepted entries and their use:

Legend:  `name_of_key: type (default)`

- `prefer_online_tts: bool (false)`: If `true` Lily will prefer an online service for Voice Synthesis.
- `prefer_online_stt: bool (false)`: If `true` Lily will prefer an online service for Speech Recognition.
- `ibm_tts_key: string (empty)`: The `key` used to send to IBM for it's online Voice Synthesis.
- `ibm_stt: dict (empty)`: Data for the ibm STT, can be found in IBM's console
  - `api_key: string (empty)`: STT's api key
  - `instance: string (empty)`: STT's instance ID ()
  - `gateway: string (empty)`: where is the STT instance located (London, Seoul, ...)
- `ibm_gateway: string (empty)`: The `gateway` which Lily will connect when using IBM's online services.
- `language: string (empty)`: Which language lily will use, if left empty will use the received from the use
- `hotword_sensitivity: float (0.45)`: The senstivity for the hotword (by default: "Lily") as defined by Snowboy (Bigger value==more easily triggered).
- `debug_record_active_speech: bool (false)`: `true` here makes Lily save an audio file of what was send last time to Speech Recognition (for Speech Recognition debugging purposes).

Note: In order to activate IBM's Voice Synthesis you need to set `ibm_tts_key`,
`ibm_gateway` and set `prefer_online_tts` to `true`, howerver, if cargo feature 
`google_tts` is defined and either `ibm_tts_key` or `ibm_tts_key` is not set 
`prefer_online_tts` is, then Google Tts will be used.In order to have IBM's voice
recognition you need to fill `ibm_stt` and have set 
`prefer_online_stt` to `true`. You can get `ibm_gateway`/`gateway`, `ibm_tts_key`/`api_key` and
from IBM after registering an account for their online services,
in their free plan you get a pretty good number of minutes per month.

Also, this file can have map keys with the name of a package, that package will
access that map while getting it's configuration, for example when package `home_assistant`
asks for `auth_key` this is what the config must have:

```yaml
home_assitant:
    auth_key: here_goes_the_actual_key
```

