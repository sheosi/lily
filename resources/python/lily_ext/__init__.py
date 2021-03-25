"""Python module connecting Python code to Lily"""

import datetime
from enum import Enum
import locale
import os
from pathlib import Path
import random
import inspect
from sys import version_info
from typing import Any, Dict, get_type_hints, Mapping, List, Optional, Tuple
from functools import reduce

from fluent.runtime import FluentBundle, FluentResource

import _lily_impl
from _lily_impl import *

# We are going to access things from the runtime
# that one else should
# pylint: disable=protected-access

_action_classes: Dict[str, Any] = {}
_signal_classes: Dict[str, Any] = {}
_query_classes:  Dict[str, Any] = {}
skills_translations = {}

#pylint: disable=invalid-name
class __ProtocolErrs:
    """A class to hold errors and warnings found while analyzing protocols"""
    errs = ""
    warns = ""

    @staticmethod
    def __add_str(probs: str, prob: str) -> str:
        if probs:
            probs += ","
        probs += prob

        return probs

    def add_error(self, prob: str):
        """Appends prob to the error string (adding a comma if needed)"""
        self.errs = self.__add_str(self.errs, prob)

    def add_warn(self, prob: str):
        """Appends prob to he warning string (adding a comma if needed)"""
        self.warns = self.__add_str(self.warns, prob)

    def has_errors(self) -> bool:
        """Returns True if any error has been found"""
        return self.errs != ""

    def has_warns(self) -> bool:
        """Returns True if any warning has been found"""
        return self.warns != ""

def __compare_class_with(cls: Any, model: Any) -> __ProtocolErrs:
    def are_arguments_optional_from(params: Mapping[str, inspect.Parameter], first: int) -> bool:
        for idx, param  in enumerate(params.values()):
            if idx >= first:
                if param.default is None:
                    return False

        return True

    res = __ProtocolErrs()

    for attr in dir(model):
        # Ignore privates, builtins and other specials
        if attr[0:2] == "__":
            continue

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

def __compare_had_errors(cls: Any, model:Any, title:str, name: str) -> bool:
    cls_err = __compare_class_with(cls, model)
    if cls_err.has_errors():
        _lily_impl.log_error(f"{title.capitalize()} {name} doesn't conform to the {title} protocol: {cls_err}. Won't be loaded")
    else:
        if cls_err.has_warns():
            _lily_impl.log_warn(f"{title.capitalize()} {name} might have some problems: {cls_err}")

    return cls_err.has_errors()

class ActionProtocol:
    """Just an example action to compare to incoming actions"""
    def trigger_action(self, context):
        """This function is called when an action is triggered. 'context' is
        filled by the signal, and mostly will have things like slots
        or other relevant data"""


def action(name: str):
    """Declares a class an action, it will be available for skills to use. To
    learn more see the ActionProtocol class. Note: the class will be checked at
    runtime and might be rejected if it doesn't conform to the ActionProtocol
    (will also be checked at compile time with mypy and Python 3.8)"""
    def inner_deco(cls: ActionProtocol):
        
        if not __compare_had_errors(cls, ActionProtocol, 'Action', name):
            _action_classes[name] = cls

        return cls

    return inner_deco


class SignalProtocol:
    """The definition of a signal. A signal react when some event has happened
       by activating an ActionSet"""
    def add_sig_receptor(self, args: Dict[str, str], intent_name: str, skill_name: str, actset: _lily_impl.PyActionSet):
        """Called by the app to add  saet of actions that should be executed in
        relation to some event"""

    #def end_load(self, curr_langs: List[str]):
    #    """Called when load has been finished, use this to do any kind of
    #    finalization, optimization or resource liberation needed. *Optional*"""


    def event_loop(self, base_context: Dict[str, str], curr_lang: str):
        """Start custom event loop, for the time being it is recomended that
        you start your own thread as Lily doesn't do it (this will change in the
        future)"""

def signal(name: str):
    """Declares a class a signal.To learn more see the SignalProtocol class.
    Note: the class will be checked at runtime and might be reject if doesn't
    conform to the SignalProtocol (will also be checked at compile time with
    mypy and Python 3.8)"""
    def inner_deco(cls):
        if not __compare_had_errors(cls, SignalProtocol, 'Signal', name):
            _signal_classes[name] = cls

        return cls

    return inner_deco


class QueryProtocol:
    pass

def query(name: str):
    """Declares a class a query, it will """
    def inner_deco(cls):
        if not __compare_had_errors(cls, QueryProtocol, 'Query', name):
            _query_classes[name] = cls

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
    """Just a small class containing a set of translations, both in current
    language and in the default one, this way we can fallback to the default one
    if something ever happens"""
    def __init__(self, current_langs: Dict[str, FluentBundle], default: FluentBundle):
        self.current_langs = current_langs
        self.default = default

def __set_translations(curr_langs_str: List[str]):
    trans_path = Path('translations')
    DEFAULT_LANG = "en-US"
    
    if trans_path.is_dir():
        lang_list = []
        for lang in trans_path.iterdir():
            if lang.is_dir():
                lang_list.append(lang.name)

        neg_langs: List[str] = _lily_impl._negotiate_lang(curr_langs_str, DEFAULT_LANG, lang_list)
        if DEFAULT_LANG not in neg_langs:
            default_lang = __gen_bundle(DEFAULT_LANG, trans_path/DEFAULT_LANG)
        else:
            default_lang = None

        def add_to_dict(d: Dict[str, FluentBundle],l: str) -> Dict[str, FluentBundle]:
            d[l] = __gen_bundle(l, trans_path/l)
            return d

        bundles: Dict[str, FluentBundle] = reduce( add_to_dict, neg_langs, {})

        skills_translations[_lily_impl._get_curr_lily_skill()] = TransPack(bundles, default_lang)

    else:
        _lily_impl.log_warn("Translations not present in " + os.getcwd())


def _gen_trans_list(trans_name: str, lang: str) -> Tuple[FluentBundle, List[Any]]:
    translations = skills_translations[_lily_impl._get_curr_lily_skill()]
    if lang in translations.current_langs:
        try:
            trans = translations.current_langs[lang].get_message(trans_name)
            translator = translations.current_langs[lang]
        except LookupError as _e:
            log_str = f"Translation '{trans_name}'  not present in selected lang"
            if translations.default:
                _lily_impl.log_warn(log_str + ", using default translation")
                trans = translations.default.get_message(trans_name)
                translator = translations.default
            else:
                _lily_impl.log_warn(log_str)
                raise
    else:
        log_str = f"Translation '{trans_name}'  not present in selected lang"
        if translations.default:
            _lily_impl.log_warn(log_str + ", using default translation")
            trans = translations.default.get_message(trans_name)
            translator = translations.default
        else:
            _lily_impl.log_warn(log_str)
            raise KeyError(log_str)

    all_trans = list(trans.attributes.values())
    all_trans.insert(0, trans.value)

    return (translator, all_trans)

def _translate_all_impl(trans_name: str, dict_args: Dict[str, Any], lang: str):
    translations, all_trans = _gen_trans_list(trans_name, lang)
    

    def extract_trans(element):
        trans, err = translations.format_pattern(element, dict_args)
        if err:
            _lily_impl.log_warn(str(err))
        return trans

    res = list(map(extract_trans, all_trans))
    return res

def _translate_impl(trans_name: str, dict_args: Dict[str, Any], lang: str):
    translations, all_trans = _gen_trans_list(trans_name, lang)
    sel_trans = random.choice(all_trans)
    trans, err = translations.format_pattern(sel_trans, dict_args)
    if err: # Note: this will only show the error for the one picked
        _lily_impl.log_warn(str(err))

    return trans


def translate_all(trans_name: str, dict_args: Dict[str, Any]):
    """Translate using 'dict_args' as context variables for them to be used
    inside Fluent. Returns a list with all possible alternatives for this
    translation"""

    return _translate_all_impl(trans_name, dict_args, dict_args["__lily_data_lang"])

def translate(trans_name: str, dict_args: Dict[str, Any]):
    """Translate using 'dict_args' as context variables for them to be used
    inside Fluent. If multiple alternatives exist returns one at random."""

    return _translate_impl(trans_name, dict_args, dict_args["__lily_data_lang"])

def answer(output: str, context: Dict[str, str]) -> Optional[_lily_impl.ActionAnswer]:
    """'output' will be returned for it to be shown directly to the user or
    voiced by the TTS engine according to what was originally used"""
    uuid = context["__lily_data_satellite"]
    if _lily_impl.has_cap(uuid, 'voice'):
        return _lily_impl.ActionAnswer.text(output)
    else:
        _lily_impl.log_error(f"Satellite '{uuid}' doesn't implement 'voice' capapbility, answer can't be sent")
        return None

def answer_audio_file(file: str, context: Dict[str, Any]) -> Optional[_lily_impl.ActionAnswer]:
    uuid = context["__lily_data_satellite"]
    if _lily_impl.has_cap(uuid, 'voice'):
        return _lily_impl.ActionAnswer.load_audio(file)
    else:
        _lily_impl.log_error(f"Satellite '{uuid}' doesn't implement 'voice', audio can't be sent")
        return None