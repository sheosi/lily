"""Features provided by the lily runtime, offer
integration with it (access to configurations, logs,
and other utilities)"""

# Note: This file is actually a stub for helping in development, it contains no
# actual functionality

# pylint: disable=unused-argument

from typing import Any, Dict, Iterator, Iterable, List, Optional, Tuple

def conf(conf_name: str) -> str:
    "Gets some conf value"
    ...

def _negotiate_lang(input_lang: str, default: str, available: List[str]) -> List[str]:
    """Given a lang id tries to get the one from list
    that is compatible otherwise it will return the default one"""
    return [""]

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

def has_cap(client: str, cap: str) -> bool:
    """Returns True if the 'client' has declared that supports 'cap'"""
    return False

class PyActionSet:
    """Represents an set of actions related to a signal"""

    def call(self, context: Dict[str, str]) -> None:
        """Calls all the actions in the set"""
        ...


class ActionContext(Iterable):
    """A dictionary replacement that only accepts str as key and value, implemented in Rust for interop.
    The only real change is that it doesn't accept None as a value, and methods are modified to reflect that"""

    @classmethod
    def fromkeys(self, iterable: Iterable[str], value: str) -> 'ActionContext':
        ...
    
    def __contains__(self, item: str) -> bool:
        """Implement 'key in ActionContext'"""
        ...

    def __delitem__(self, key):
        """Implement del"""
        ...

    def __iter__(self) -> Iterator[Tuple[str,str]]:
        ...
    
    def __eq__(self, other: Any) -> bool:
        """Implement ActionContext == object"""
        ...

    def __getitem__(self, key: str) -> str:
        """Implement '[]'"""
        ...

    def __len__(self) -> int:
        """Implement len(ActionContext)"""
        ...

    def __lt__(self, other: 'ActionContext') -> bool:
        """Implement '<' '>' for two ActionContexts"""
        ...

    def __setitem__(self, key: str, item: str):
        """Implement '[]='"""
        ...

    def __str__(self) -> str:
        """Implement str(ActionContext)"""
        ...

    def __repr__(self) -> str:
        ...
    
    def __reversed__(self) -> Iterator[Tuple[str,str]]:
        ...

    def clear(self):
        ...

    def copy(self) -> 'ActionContext':
        ...

    def get(self, key: str) -> str:
        ...

    def has_key(self, k: str) -> bool:
        ...

    def items(self) -> 'ActionContextItemsView':
        ...
    
    def keys(self) -> 'ActionContextKeysView':
        ...
    
    def pop(self, key: str, default: Optional[str]) -> str:
        ...

    def popitem(self) -> Tuple[str, str]:
        ...

    def setdefault(self, key: str, default: str) -> str:
        ...

    def update(self, *args, **kwargs):
        ...

    def values(self) -> 'ActionContextValuesView':
        ...

class ActionContextItemsView(Iterable):
    def __contains__(self, item: str) -> bool:
        """Implement 'key in ActionContextItemsView'"""
        ...

    def __iter__(self) -> Iterator[Tuple[str,str]]:
        ...

    def __len__(self) -> int:
        """Implement len(ActionContextItemsView)"""
        ...

    def __reversed__(self) -> Iterator[Tuple[str,str]]:
        ...

class ActionContextKeysView(Iterable):
    def __contains__(self, item: str) -> bool:
        """Implement 'key in ActionContextKeysView'"""
        ...

    def __iter__(self) -> Iterator[str]:
        ...

    def __len__(self) -> int:
        """Implement len(ActionContextKeysView)"""
        ...

    def __reversed__(self) -> Iterator[str]:
        ...

class ActionContextValuesView(Iterable):
    def __contains__(self, item: str) -> bool:
        """Implement 'key in ActionContextValuesView'"""
        ...

    def __iter__(self) -> Iterator[str]:
        ...

    def __len__(self) -> int:
        """Implement len(ActionContextValuesView)"""
        ...

    def __reversed__(self) -> Iterator[str]:
        ...

class ActionAnswer():
    @staticmethod
    def load_audio(path: str)-> 'ActionAnswer':
        ...

    @staticmethod
    def text(text: str) -> 'ActionAnswer':
        ...