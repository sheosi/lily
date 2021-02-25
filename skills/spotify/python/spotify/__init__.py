from lily_ext import action, answer, conf, translate

@action(name = "default_action")
class spotify:

    def __init__(self):
        pass

    def trigger_action(self, context):
        if context["intent"] == "example":
            answer("$example_translation_say", context)
    