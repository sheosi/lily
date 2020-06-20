# Lily's architecture

Since I want Lily to be really flexible is actual architecture is not just an
straightforward Voice -> Stt -> Nlu -> Tts, but rather is more complex, but 
at the same time I think it makes a lot of sense.

Lily having multiple signals has been inspired by the Kalliope project
(another assistant), woudln't have it if Kalliope didn't exist.

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