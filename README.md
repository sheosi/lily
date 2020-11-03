# lily

A local open-source voice assistant with an NLU

Lily is written in Rust + Python.

## Obtaining Lily
Lily uses git [LFS](https://git-lfs.github.com/) which means it needs to be
cloned with that installed and ready beforehand.

First install git LFS, for Debian/Ubuntu it is:

```
sudo apt install git-lfs
```

Then, no matter which OS you are under you need to initialize git LFS:

```
git lfs install
```

Now, you can clone the repo:

```
git clone https://github.com/sheosi/lily
```

Alternatively, if you already have cloned the repository but did not had lfs 
installed, on the root folder of this folder do:

```
git lfs pull
```

And you'll be good to go.

## Building

### Dependencies

#### Dependencies needed at compile time and runtime:

```
- libssl-dev
- libasound2-dev
- libpocketsphinx-dev
- libsphinxbase-dev
- python3-all-dev
- libopus-dev
- clang
- libgsl-dev
- vorbis-utils (needs ogg123 while git rodio isn't working properly)
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

Recommended way of installing them:
```
pip3 install --user snips-nlu
pip3 install --user fluent.runtime
```

#### Install english module for NLU:

`snips-nlu download en`

#### Install spanish module for NLU:

`snips-nlu download es`

### Build process
Once you have at least the compile time dependencies you can compile lily, you'll
need [Rust](https://www.rust-lang.org/) and cargo (bundled alongside Rust) for this.

`cargo build`


## Features

- [x] Overall shell: some user interfaces, stt, tts ...
- [x] Intent and slot parsing
- [x] Multilanguage
- [x] Client/Server architecture
- [ ] Question answering
- [ ] Semantic parsing
- [ ] Interactivity (asking for something after the initial trigger)

## Where will it run?
Lily is meant to be run on-device (mostly) even on constrained hardware like a Raspberry, of course, it will still work on standard PCs and more powerful hardware.

## Current state:

The shell (the voice part and interfaces) it's in a pretty decent shape, though needs testing.
The AI itself however, is pretty rough, it's only capable of basic triggering of actions.