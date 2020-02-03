from fluent.runtime import FluentBundle, FluentResource
import _lily_impl
from pathlib import Path
import os
#import time
import datetime
import locale

action_classes = {}
packages_translations = {}

#locale.setlocale(locale.LC_ALL, 'es_ES.UTF-8')

def action(name):
    def inner_deco(cls):
        action_classes[name] = cls
        return cls

    return inner_deco

def __set_translations(curr_lang_str):
    translations = FluentBundle([curr_lang_str])
    packages_translations[_lily_impl.__get_curr_lily_package()] = translations
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
        log_warn("Translations not present in " + os.getcwd())


def _translate_impl(trans_name, dict_args):
    translations = packages_translations[_lily_impl.__get_curr_lily_package()]
    return translations.format_pattern(translations.get_message(trans_name).value, dict_args)

def translate(trans_name, dict_args):
    if trans_name[0] == '$':
        (what_to_say,_) = _translate_impl(trans_name[1:], dict_args)
    else:
        what_to_say = trans_name

    return what_to_say

def answer(output):
    _lily_impl._say(output)


@action(name = "say")
class Say():
    def trigger_action(args):
        answer(translate(args, {}))

@action(name = "play_file")
class PlayFile():
    def trigger_action(args):
        _lily_impl.__play_file(args)