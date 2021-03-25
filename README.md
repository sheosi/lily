# lily

A local open-source voice assistant with an NLU

Lily is written in Rust + Python.

## Obtaining Lily
Lily uses git [LFS](https://git-lfs.github.com/) which means it needs to be
cloned with that installed and ready beforehand.

First install git LFS, for Debian/Ubuntu it is:

```shell
sudo apt install git-lfs
```

Then, no matter which OS you are under you need to initialize git LFS:

```shell
git lfs install
```

Now, you can clone the repo:

```shell
git clone https://github.com/sheosi/lily
```

Alternatively, if you already have cloned the repository but did not had lfs 
installed, on the root folder of this folder do:

```shell
git lfs pull
```

And you'll be good to go.

## Building

### Dependencies

#### Dependencies needed at compile time and runtime:

*On Debian:*
```shell
sudo apt install libssldev libasound2-dev libpocketsphinx-dev libsphinxbase-dev python3-all-dev libopus-dev clang libgsl-dev
```

*Optional* dependency for feature `extra_langs_tts` (Languages not provided by Pico Tts for local Tts):
*Debian*
```shell
sudo apt install libespeak-ng-dev
```

Note: The first time that you use a language it needs to be downloaded by the NLU, so it needs internet at that time. Also, installing them as system would make this download fail, and you would need to install the languages on your own, for english: `snips-nlu download en`

### Build process
Once you have at least the compile time dependencies you can compile lily, you'll
need [Rust](https://www.rust-lang.org/) and cargo (bundled alongside Rust) for this.

`cargo build`

### Debian package
This repository can make a Debian package, however it is still dependent on 
`snips-nlu` and `fluent.runtime` packages being installed and it does not 
install them on it's own.

To generate the Debian package.

```shell
cargo install cargo-deb
cargo deb
```

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
It now has a kind of client/server architecture, this is pretty new and needs tons of testing but should bring this project pretty far.