import datetime
from typing import Any, Dict
from lily_ext import action, translate, answer, answer_audio_file
import _lily_impl

@action(name="say_date_time")
class SayTime:
    def trigger_action(self, context):
        name = context["intent"]["name"]
        if name == "say_time":
            formatstr = "time_format"
        elif name == "say_date":
            formatstr = "date_format"
        return answer(datetime.datetime.now().strftime(translate(formatstr, context)), context)

@action(name="base_answers")
class BaseAnswers:
    def trigger_action(self, context):
        name = context["intent"]["name"]
        if name == "say_hello":
            return answer(translate("say_hello_i18n", context), context)
        if name == "say_name":
            return answer(translate("say_name", context), context)
        if name == "repeat":
            return answer(translate("say_repeat", context), context)

@action(name="event_handling")
class EventHandling:
    def trigger_action(self, context):
        name = context["event"]["name"]
        if name == "lily_start":
            return answer(translate("lily_start", context), context)
        if name == "init_reco":
            return answer_audio_file("sounds/beep.ogg",context)
        if name == "unrecognized":
            return answer(translate("lily_unknown", context), context)
        if name == "empty_reco":
            return answer_audio_file("sounds/end_recognition.ogg", context)