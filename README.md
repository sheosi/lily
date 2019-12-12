# lily

A local open-source voice assistant with an NLU

Lily is written in Rust + Python.

## Lily vs other assistants

### Vs Alexa (or Google Home, Siri ...)

Main question I get: Why would I use Lily before Alexa? Short answer: most probably you wouldn't, can't lie about that. Those are pretty great products, sold with pretty great hardware for cheap (specially if you get a discounted price) and the service itself is free (something which otherwise would be a pay by month service). Now, there are some concerns with those assistants:

- *Can't be used offline:* Those services run in the cloud (which is why are so cheap), so this means no internet = no assistant, this might be a nuissance for some (if you depend a lot on your assistant then you better make sure your internet connection is reliable), but for some others this is a deal breaker, as there some situations in which internet conection is unreliable or straight up impossible (think of old villages or someone who goes trekking).

- *They can record (and store) everything:* every second it's on it's recording, tests seem to indicate that meanwhile it's not "active listening" it's not sending anything, but there's nothing telling us that the device is not storing whatever it hears and sending it later (in a timer fashion or on demand). However, there've been cases where the devices have had "false positives" and started listening on their own, this means that people have been recorded in all kind moments, even in very very very private ones. Also, all of this means that children get recorded and sent too, which is understable for them but leaves me kind of unconfortable with that situation.

### Vs Other non-local open-source assistants

This one is pretty much the same as above with the premise that your data won't be sold to anyone, and it can be verified by checking the source code, however their policy states that your data CAN BE SOLD OUT, which is upsetting and and a little terrifying.


### Vs Other local assistants

This only leaves us with local assistants. Most of them are open-source, but they share various things: they all use Python as the primary language, they use regex for parsing user requests, and they all use pocketsphinx.

- *Python:* Python is a great language, is easy, people know it and there's a ton of learning material out there (plus it has pretty interesting semantics and some integration of functional programming), but it's not fast (by any means, as it's interpreted) and most important real Python multithreading it's impossible (because of GIL), all of this makes people using python on top of other compiled languages.

- *Regex:* Using regular expressions seems okay for detecting user intention but there here two giveaways: a) the developer must take into account everything that the user might say (pretty hard, us humans are known for being unpredictable), b) typo (yourself or the voice recognition system) any word, and you are screwed which wouldn't be so bad if...

- *PocketSphinx:* pocketsphinx is fast and easy to integrate, but it's WER (Word Error Rate, how much it misses) it's terrible, in some comparisons I've seen it arund 30%, while Amazon and other online services tend to be around 10% (I'd say even less).

Now, there's a local assistant called Snips (which is thanks to whom we get our NLU, Natural Language Understanding, do check it out it's pretty interesting), but it's only partially open-source (they are trying to sell it after all), and it feels much more directed towards businesses rather than more general usage.


Knowing that I wouldn't be satisfied with anything that existed I got to make my own.

## Why...

### Why Rust + Python?

Rust is safe, and compiled (and it's ecosystem and community it's just plain awesomeness), it makes a great "core" language, but distributing modules with it means that dynamic libraries would get distributed, not only this is a bit iffy but it would mean architecture and platform lock-in to some that we establish, this would be a problem for people running it in other architectures or if we expanded OSs later, not to mention the nuissance of having to compile several libraries for different platforms.

There enters Python, an very extended language which can be interpreted in any architecture just the same, and since it is used in 'extensions', this is no critic code and the potential slow-down is non-existant.

Also: I wanted to use Rust to have a project to work on.

### Why PocketSphinx?

You roasted PocketSphinx earlier, how come you are using it? Well... you caught me, the truth is that PocketSphinx is super easy to embed, however one goal of this project is to test and see if other, more robust alternatives are viable (Kaldi and Wav2Letter).


### Why an NLU? (and what it is?)
As I said earlier, regexes just don't cut it for this task, so we need an AI to analyze which of ther orders we know is the one which possibly the user wanted. We call this an NLU (Natural Language Understanding), and we are using one from Snips.ai.


## Current features

- Support for english (en-US) and spanish (es-ES)
- Language negotiation for supported languages
- NLU
- Description of signals and actions by Yaml file and training incorporated


## Where will it run?
Lily is meant to be run on-device (mostly) even on constrained hardware like a Raspberry, of course it will still work on standard PCs and more powerful hardware.

## Runtime Dependencies:
Lily needs these at runtime:

Snips nlu python module (sudo pip3 install snips-nlu)
PocketSphinx 
Sphinxad
Python 
sox (binary) -> for gtts
libespeak-ng