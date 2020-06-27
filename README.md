# lily

A local open-source voice assistant with an NLU

Lily is written in Rust + Python.

## Building

### Dependencies

#### Dependencies needed at compile time and runtime:

```
- libssl-dev
- libasound2-dev
- libpocketsphinx-dev
- libsphinxbase-dev
- (also needs sphinxad, but is the same pacakge on debina)
- python3-all-dev
- libopus-dev
- clang
- libgsl-dev
```

Optional dependency for feature `extra_langs_tts` (Languages not provided by Pico Tts for local Tts):
```
- libespeak-ng-dev
```

#### Python dependencies (needed for runtime):

```
- snips-nlu
- fluent.runtime
```

#### Install english module for NLU:

`sudo snips-nlu download en`

#### Install spanish module for NLU:

`sudo snips-nlu download es`

### Build process
Once you have at least the compile time dependencies you can compile lily, you'll
need [Rust](https://www.rust-lang.org/) and cargo (bundled alongside Rust) for this.

`cargo build`


## Features

- [x] Overall shell: some user interfaces, stt, tts ...
- [x] Intent and slot parsing
- [x] Multilanguage
- [ ] Question answering
- [ ] Semantic parsing
- [ ] Interactivity (asking for something after the initial trigger)

## Where will it run?
Lily is meant to be run on-device (mostly) even on constrained hardware like a Raspberry, of course, it will still work on standard PCs and more powerful hardware.

## Current state:

The shell (the voice part and interfaces) it's in a pretty decent shape, though needs testing.
The AI itself however, is pretty rough, it's only capable of basic triggering of actions.