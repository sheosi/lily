from lily_ext import action, translate, answer
import datetime

@action(name = "say_date_time")
class SayTime():
    def trigger_action(args):
        answer(datetime.datetime.now().strftime(translate(args, {})))