from lily_ext import action

@action(name = "say_date_time")
class SayTime():
    def trigger_action(args):
        if args[0] == '$':
            (what_to_say,_) = translate(args[1:], {})
        else:
            what_to_say = args

        _lily_impl._say(datetime.datetime.now().strftime(what_to_say))