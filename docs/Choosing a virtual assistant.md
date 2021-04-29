# Choosing a virtual assistant

Maybe you are wondering whether Lily or another asssistant might be what you want
here are some questions to help you decide

1. Do you mind being recorded and your profile used heavily as marketing? -> NO: Amazon Alexa, Google Assistant.
2. Are you into Apple ecosystem (and kinda don't mind being recorded for internal purposes)? -> YES: Siri.
3. Do you mind your voice assistant being over the internet (and still having the possibility for some of your data to be sold, more info below) -> NO: Mycroft.
5. You (and your environment) talk only in English? -> YES: Almond.
6. You only want to talk to IoT (home assistant or others?) and you are tech-savvy -> YES: Rhasspy.
7. Otherwise you might want to give Lily a hand :).

From all of the alternatives to the established ones (Alexa and Google assistant)
the ones I'd actually recommend to take a look at (other than helping with Lily 😆)
are Almond and Mycroft.

Almond is interesting but is only text based, if you want
voice then you will have to look for that yourself, and that brings it's own
problems, good news is that you can easily have a local version, tough if you
speak somethig that happens not be English, then forget about it.

Now, Mycroft is much more insteresting, as it is more established, has some
language support and has the widest community out of all the open source ones.
While technically I don't agree with how they seem to structure things, my main
gripe with them comes with their server called 'home'. This server can't be 
avoided as of the time of this writing and does some operations like STT and  
TTS, (and while those two are apparently optional, you can rely on others, some
info might be sent regardless maybe as part of their OpenAudio initiative), also
they store some settings and let you configure some things. The (bad) icing on
the cake comes when as part of their privacy policy they explicitly tell you
that your data can be lent to someone performing some function for them and that
an anonimized version of your data could be sold, and that everything that they
have could be in hands of someone else if they bought Mycroft, kudos to them for
being explicit, but the situation here does not feel that different from that of
Amazon or Google regarding privacy.