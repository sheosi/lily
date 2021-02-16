import datetime
from typing import Any, Dict
from lily_ext import action, translate, answer
import _lily_impl

@action(name="say_date_time")
class SayTime:
    def trigger_action(self, context):
        if context["intent"] == "say_time":
            formatstr = "$time_format"
        elif context["intent"] == "say_date":
            formatstr = "$date_format"
        answer(datetime.datetime.now().strftime(translate(formatstr, context)), context)

@action(name="base_answers")
class BaseAnswers:
    @staticmethod
    def send_audio(file: str, context: Dict[str, Any]):
        uuid = context["__lily_data_satellite"]
        if _lily_impl.has_cap(uuid, 'voice'):
            _lily_impl._play_file(uuid, file)
        else:
            _lily_impl.log_error(f"Satellite '{uuid}' doesn't implement 'voice', audio can't be sent")

    def trigger_action(self, context):
        # Events
        if context["intent"] == "lily_start":
            answer("$lily_start", context)
        if context["intent"] == "init_reco":
            self.send_audio("sounds/beep.ogg",context)
        if context["intent"] == "unrecognized":
            answer("$lily_unknown", context)
        if context["intent"] == "empty_reco":
            self.send_audio("sounds/end_recognition.ogg", context)

        # Proper intents
        if context["intent"] == "say_hello":
            answer("$say_hello_i18n", context)
        if context["intent"] == "say_name":
            answer("$say_name", context)
        if context["intent"] == "repeat":
            answer("$$say_repeat", context)
        
