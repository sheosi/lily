"""Features provided by the lily runtime, offer
integration with it (access to configurations, logs,
and other utilities)"""

# Note: This file is actually a stub for helping in development, it contains no
# actual functionality

# pylint: disable=unused-argument

from typing import Dict, List

def _say(text: str):
    "Sends the text to be said or shown (depending on the circumstances)"

def conf(conf_name: str) -> str:
    "Gets some conf value"
    ...

def _negotiate_lang(input_lang: str, default: str, available: List[str]) -> str:
    """Given a lang id tries to get the one from list
    that is compatible otherwise it will return the default one"""
    return ""

def log_info(text: str):
    """Writes into the log as info"""
    ...

def log_warn(text: str):
    """Writes into the log as a warning"""
    ...

def log_error(text: str):
    """Writes into the log as error"""
    ...

def _get_curr_lily_package() -> str:
    """Returns the name of the package being executed right now"""
    return ""

def _play_file(file_name: str):
    """Plays a music file"""
    ...

class PyActionSet:
    """Represents an set of actions related to a signal"""

    def call(self, context: Dict[str, str]) -> None:
        """Calls all the actions in the set"""
        ...