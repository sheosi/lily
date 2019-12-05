import lily_sdk
import lily
	
if False:
    @lily_sdk.action(name = "init_reco")
    class Perfume():
        def trigger_action():
        	lily.say("Dime")

    @lily_sdk.action(name = "lily_start")
    class StartAction():
    	def trigger_action():
    		lily.say("Lily lista para la acción")

    @lily_sdk.action(name = "say_hello")
    class SayHello():
    	def trigger_action():
    		lily.say("Hola, querido")

    @lily_sdk.action(name = "unrecognized")
    class Unrecognized():
        def trigger_action():
            lily.say("Lo siento, no te he entendido")

    @lily_sdk.action(name = "set_timer")
    class SetTimer():
        def trigger_action():
            lily.say("Claro, te avisaré para entonces")

    @lily_sdk.action(name = "what_date")
    class WhatTime():
        def trigger_action():
            lily.say(date.today().strftime("%d de %B del %Y"))

    @lily_sdk.action(name = "what_hour")
    class WhatTime():
        def trigger_action():
            lily.say(datetime.now().strftime("Son las %H y %M"))

    @lily_sdk.action(name = "turn_on_lights")
    class TurnOnLights():
        def trigger_action():
            lily.say(datetime.now().strftime("Son las %H y %M"))

    @lily_sdk.action(name = "tell_a_joke")
    class TellAJoke():
        def trigger_action():
            lily.say("Hmm. Preguntale a Alexa que sabe más")

@lily_sdk.action(name = "say")
class Say():
    def trigger_action():
        lily.say("Testeando")