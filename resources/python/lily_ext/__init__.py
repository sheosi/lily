import datetime
from enum import Enum
import locale
import os
from pathlib import Path
import random
import inspect
from sys import version_info
from typing import Any, Dict, get_type_hints, Mapping, List, Optional, Tuple

from fluent.runtime import FluentBundle, FluentResource

import _lily_impl
from _lily_impl import conf

# We are going to access things from the runtime
# that one else should
# pylint: disable=protected-access

_action_classes: Dict[str, Any] = {}
_signal_classes: Dict[str, Any] = {}
packages_translations = {}

class __InterfaceErrs:
    errs = ""
    warns = ""

    @staticmethod
    def __add_str(probs: str, prob: str) -> str:
        if probs:
            probs += ","
        probs += prob

        return probs

    def add_error(self, prob: str):
        self.errs = self.__add_str(self.errs, prob)

    def add_warn(self, prob: str):
        self.warns = self.__add_str(self.warns, prob)

    def has_errors(self) -> bool:
        return self.errs != ""

    def has_warns(self) -> bool:
        return self.warns != ""

def __compare_class_with(cls: Any, model: Any) -> __InterfaceErrs:
    def are_arguments_optional_from(params: Mapping[str, inspect.Parameter], first: int) -> bool:
        for idx, param  in enumerate(params.values()):
            if idx >= first:
                if param.default is None:
                    return False

        return True

    res = __InterfaceErrs()

    for attr in model:
        mod_attr = getattr(model, attr)
        cls_attr = getattr(cls, attr, None) # Need the none, might not have it
        if callable(mod_attr):
            if cls_attr is None:
                res.add_error(f"lacks method {mod_attr}")
            elif not callable(cls_attr):
                res.add_error(f"has an attribute called {mod_attr}, but is not a method")
            else:
                mod_sig = inspect.signature(mod_attr)
                cls_sig = inspect.signature(cls_attr)
                n_params_mod = len(cls_sig.parameters)
                n_params_cls = len(mod_sig.parameters)

                if n_params_cls < n_params_mod :
                    res.add_error(f"method {mod_attr} too few arguments")

                elif n_params_cls > n_params_mod and not are_arguments_optional_from(cls_sig.parameters, n_params_mod):
                    res.add_error(f"method {mod_attr} too many arguments and they are not optional")

        else:
            if cls_attr is None:
                res.add_error(f"lacks attribute {cls_attr}")


    return res


class ActionProtocol:
    """Just an example action to compare to incoming actions"""
    def trigger_action(self, args, context):
        pass

def action(name: str):
    def inner_deco(cls: ActionProtocol):
        cls_err = __compare_class_with(cls, ActionProtocol)
        if cls_err.has_errors():
            _lily_impl.log_error(f"Action {name} doesn't conform to the action protocol: {cls_err}. Won't be loaded")
        else:
            if cls_err.has_warns():
                _lily_impl.log_warn(f"Action {name} might have some problems: {cls_err}")

            _action_classes[name] = cls

        return cls

    return inner_deco


class SignalProtocol:
    def add_sig_receptor(self, args: Any, skill_name: str, pkg_name: str, actset: _lily_impl.PyActionSet):
        pass

    def end_load(self):
        pass

    def event_loop(self, base_context: Dict[str, str], curr_lang: str):
        pass

def signal(name: str):
    def inner_deco(cls):
        cls_err = __compare_class_with(cls, SignalProtocol)
        if cls_err.has_errors():
            _lily_impl.log_error(f"Signal {name} doesn't conform to the signal protocol: {cls_err}. Won't be loaded")
        else:
            if cls_err.has_warns():
                _lily_impl.log_warn(f"Signal {name} might have some problems: {cls_err}")

            _signal_classes[name] = cls
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

        neg_lang = _lily_impl._negotiate_lang(curr_lang_str, DEFAULT_LANG, lang_list)
        if neg_lang != DEFAULT_LANG:
            default_lang = __gen_bundle(DEFAULT_LANG, trans_path/DEFAULT_LANG)
        else:
            default_lang = None

        packages_translations[_lily_impl._get_curr_lily_package()] = TransPack(__gen_bundle(neg_lang, trans_path/neg_lang), default_lang)

    else:
        _lily_impl.log_warn("Translations not present in " + os.getcwd())


def _gen_trans_list(trans_name: str) -> Tuple[FluentBundle, List[Any]]:
    translations = packages_translations[_lily_impl._get_curr_lily_package()]
    try:
        trans = translations.current.get_message(trans_name)
        translator = translations.current
    except LookupError as _e:
        log_str = f"Translation '{trans_name}'  not present in selected lang"
        if translations.default:
            _lily_impl.log_warn(log_str + ", using default translation")
            trans = translations.default.get_message(trans_name)
            translator = translations.default
        else:
            _lily_impl.log_warn(log_str)
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
    if err: # Note: this will only show the error for the one picked
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
    def trigger_action(self, args: Dict[str, str], context: Dict[str, str]):
        answer(translate(args, context))

@action(name = "play_file")
class PlayFile():
    def trigger_action(self, args, _context):
        _lily_impl._play_file(args)
