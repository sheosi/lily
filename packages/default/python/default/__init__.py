import datetime
from lily_ext import action, translate, answer

@action(name="say_date_time")
class SayTime:
    def trigger_action(self, args, context):
        answer(datetime.datetime.now().strftime(translate(args, context)))