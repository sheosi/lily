# Cargo Features

Lily has a set of cargo features which change some details like which software
is used for Voice Synthesis or for Speech Recognition, among others. Here's
everyone and what they do:

### For users

Optional features that a user might like:

- `extra_langs_tts`: Enable espeak for those languages that aren't supported by 
Pico. Be aware that Espeak's license is GPL3, which may be problematic in 
some situations.

- `google_tts`: Enable Google's TTS as online TTS which is used if IBM's data
is not provided.

### For developers

Features that are in development, in the future they are expected to be the
default.

- `devel_cpal_rec`: Uses a custom-made _cpal_-based module for recording instead
of Sphinxad, though Sphinxad might be faster (at least it's pulseaudio
implementation is very simple), not depending on it, makes building Lily easier,
and makes us less dependent on pocketsphinx (which might be removed from being
by default in the future).

- `devel_deepspeech`: Use _deeepspeech_ as part of the voice recognition system.
Still in testing, and it's actual future will depend on how it performs on
systems like the latest Raspberry pi. Note: In the future I would also like to
try wav2letter, which is said to be the fastest AI-based Voice Recognition
software.


- `devel_rasa_nlu`: Use _rasa_ as the NLU, instead of Snips, Rasa is much less
constrained when it comes to languages, can be installed with "pip --user"
effortlessly (Snips uses a custom script to install languages, this script does
not work with "--user"), and is going to be mantained (after the buy out of 
Snips, this is not so clear for Snips NLU). Note: DeepPavlov could also be
pretty interesting.

