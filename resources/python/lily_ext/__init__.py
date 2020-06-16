from fluent.runtime import FluentBundle, FluentResource
import _lily_impl
from _lily_impl import conf
from pathlib import Path
import os
import datetime
import locale
import random
from enum import Enum
from typing import Tuple, List, Any

action_classes = {}
packages_translations = {}

def action(name: str) :
    def inner_deco(cls):
        action_classes[name] = cls
        return cls

    return inner_deco

def __gen_bundle(lang: str, trans_path: Path) -> FluentBundle:
    bundle = FluentBundle([lang])
    for trans_file in trans_path.glob("*.ftl"):
        if trans_file.is_file():
            trans_ftl = ""
            with trans_file.open() as f:
                trans_ftl = f.read()
            bundle.add_resource(FluentResource(trans_ftl))

    return bundle

class TransPack:
    def __init__(self, current, default):
        self.default = default
        self.current = current

def __set_translations(curr_lang_str: str):
    trans_path = Path('translations')
    DEFAULT_LANG = "en-US"
    if trans_path.is_dir():
        
        lang_list = []
        for lang in trans_path.iterdir():
            if lang.is_dir():
                lang_list.append(lang.name)

        neg_lang = _lily_impl.__negotiate_lang(curr_lang_str, DEFAULT_LANG, lang_list)
        if neg_lang != DEFAULT_LANG:
            default_lang = __gen_bundle(DEFAULT_LANG, trans_path/DEFAULT_LANG)
        else:
            default_lang = None

        packages_translations[_lily_impl.__get_curr_lily_package()] = TransPack(__gen_bundle(neg_lang, trans_path/neg_lang), default_lang)

    else:
        _lily_impl.log_warn("Translations not present in " + os.getcwd())


def _gen_trans_list(trans_name: str) -> Tuple[FluentBundle, List[Any]]:
    translations = packages_translations[_lily_impl.__get_curr_lily_package()]
    try:
        trans = translations.current.get_message(trans_name)
        translator = translations.current
    except LookupError as e:
        _lily_impl.log_warn("Translation not present in selected lang")
        if translations.default:
            trans = translations.default.get_message(trans_name)
            translator = translations.default
        else:
            raise

    all_trans = list(trans.attributes.values())
    all_trans.insert(0, trans.value)

    return (translator, all_trans)

def _translate_all_impl(trans_name, dict_args):
    translations, all_trans = _gen_trans_list(trans_name)
    

    def extract_trans(element):
        trans, err = translations.format_pattern(element, dict_args)
        if err:
            _lily_impl.log_warn(str(err))
        return trans

    res = list(map(extract_trans, all_trans))
    return res

def _translate_impl(trans_name, dict_args):
    translations, all_trans = _gen_trans_list(trans_name)
    sel_trans = random.choice(all_trans)
    trans, err = translations.format_pattern(sel_trans, dict_args)
    if err: # NOte this will only show the error for the one picked
            _lily_impl.log_warn(str(err))

    return trans


def translate_all(trans_name, dict_args):
    if trans_name[0] == '$':
        what_to_say = _translate_all_impl(trans_name[1:], dict_args)
    else:
        what_to_say = [trans_name]

    return what_to_say

def translate(trans_name, dict_args):
    """Returns a translated element, if multiple exist one at random is selected"""
    if trans_name[0] == '$':
        what_to_say = _translate_impl(trans_name[1:], dict_args)
    else:
        what_to_say = trans_name

    return what_to_say

def answer(output):
    _lily_impl._say(output)


@action(name = "say")
class Say():
    def trigger_action(args, context):
        answer(translate(args, context))

@action(name = "play_file")
class PlayFile():
    def trigger_action(args, _context):
        _lily_impl.__play_file(args)
