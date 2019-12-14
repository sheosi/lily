import lily_sdk
import lily
	
@lily_sdk.action(name = "say")
class Say():
    def trigger_action(args):
        lily.say(args)