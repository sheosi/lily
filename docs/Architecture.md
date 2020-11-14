# Lily's architecture

Since I want Lily to be really flexible is actual architecture is not just an
straightforward Voice -> Stt -> Nlu -> Tts, but rather is more complex, but
at the same time I think it makes a lot of sense.

Lily having multiple signals has been inspired by the Kalliope project
(another assistant), woudln't have it if Kalliope didn't exist.

## Client/Server

Recently, Lily has been transformed into a client/server architecture, with the
clients being called satellites and the server being called... server, it doesn't
have any special name.

Client and Server communicate through MQTT, and most of the heavy lifting is
done in the server while the client can be pretty dumb (it only executes some
commands received from the server and performs hotword check and VAD analysis if
using voice commands).

It is planned that the client will announce what it can and can't do
through the 'capabilities' system, and the server and it's plugins might exploit
those to perform actions and interact with the user. Those 'capabilities' will be
optional protocols and will be independent from each other, this way a client
won't be restrained on form, functionalities or complexity. Other than the basic
communication with the server everything will be considered a capability: Voice
interaction, text interaction, showing images, etc.

### Reasoning behind the current architecture

There's already the Hermes protocol, should we use it? Hermes basically
transforms a voice assistant into a set of microservices working together, while
it makes some sense when you think about scalability (specially with local
Speech Recognition and Speech Synthesis) and makes the overall build more
customizable, however it makes deployment much more difficult(think about
making sure that the process is executing and is not dead because of an error),
and on Android/iOS targets we would need a different direction completely, there's
also a potential bigger overhead since there's packing and transmission of data
across processes to do, and the OS putting to sleep waking up processes
(AFAIK calling the OS is a sure fire way of hurting performance).

So should we use the Hermes protocol? (since there are components out there
ready?) No. Lily's objective is more akin to that of Google Now and Siri in that
they will do more than just voice, this means we will have to deviate from the Snips Hermes
protocol,though i might follow it were it makes sense. Also, since I think that
being offline capable at all times is important, I envision one having multiple
Lilys that at times might synchronize (like having one on your phone and
another on your house), something which AFAIK Hermes doesn't support,


## Core

Since Lily's primary objective is to do some actions when some signals are
received (whether they are voice or otherwise), the core is actually a system
mapping signals and parameters to those signals to actions and parameters to
those actions. A set of signals, and actions called by those signals is called a
'skill'.

In this case voice is just another signal.

## Signals

As we said voice is another signal (called 'order') , though to be fair is kind
of a special signal as, both, 'order' and 'event' (a signal which is used to
signal all kind of things happening from code: start active listening, error ...),
are treated differently in the code, specially 'event' which is used in other signals
and other parts. For the time being the only way to add new signals is by Python
(though that is not yet fully implemented, but it shoudln't take long).

A skill having multiple signals will activate all of it's actions whenever any
of the signals are activated.

## Actions

Actions are anything that react to some signal being sent. A skill hacing multiple
actions will make all of them to activate whenver any signal is activated.

## Interface

The 'order' signal, can change it interacts with the users and uses something
I call an interface (the trait name is 'UserInterface'), again this interface
is intended to be anything from a voice interface directly to an API interface.
