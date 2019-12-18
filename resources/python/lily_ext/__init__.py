from fluent.runtime import FluentBundle, FluentResource
import _lily_impl
from pathlib import Path

action_classes = {}

def action(name):
    def inner_deco(cls):
        action_classes[name] = cls
        return cls

    return inner_deco

def __set_translations(curr_lang_str):
    global translations 
    translations = FluentBundle([curr_lang_str])
    trans_path = Path('translations')
    if trans_path.is_dir():
        
        lang_list = []
        for lang in trans_path.iterdir():
            if lang.is_dir():
                lang_list.append(lang.name)

        neg_lang = _lily_impl.__negotiate_lang(curr_lang_str, lang_list)

        curr_trans_path = trans_path / neg_lang

        for trans_file in curr_trans_path.glob("*.ftl"):
            if trans_file.is_file():
                trans_ftl = ""
                with trans_file.open() as f:
                    trans_ftl = f.read()
                translations.add_resource(FluentResource(trans_ftl))
    else:
        print("Translations not present")


def translate(trans_name, dict_args):
    return translations.format_pattern(translations.get_message(trans_name).value, dict_args)


@action(name = "say")
class Say():
    def trigger_action(args):

        if args[0] == '$':
            (what_to_say,_) = translate(args[1:], {})
        else:
            what_to_say = args

        _lily_impl._say(what_to_say)