"""Features provided by the lily runtime, offer
integration with it (access to configurations, logs,
and other utilities)"""

# Note: This file is actually a stub for helping in development, it contains no
# actual functionality

# pylint: disable=unused-argument

from typing import Any, Dict, Iterator, Iterable, List, Optional, Tuple, Union

def conf(conf_name: str) -> Any:
    """Gets some conf value, can contain '/' which will be interpreted as
    sub dictionaries"""
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

def has_cap(client: str, cap: str) -> bool:
    """Returns True if the 'client' has declared that supports 'cap'"""
    return False

def add_entity_value(entity: str, value: str, langs: Optional[List[str]]):
    """Dynamically adds a value to an entity, the NLU will be recompiled"""
    ...

def add_task(query: str, action: str):
    """Adds a new task, this task will poll the query regularly and perform the
    action. Both are the names of a query and an action respectively and both 
    have to be from the same skill"""
    ...

class PyActionSet:
    """Represents an set of actions related to a signal"""

    def call(self, context: Dict[str, str]) -> None:
        """Calls all the actions in the set"""
        ...

class DynamicDict(Iterable):
    """A dictionary replacement that only accepts str as key and str, another DynamicDict or a float as values, implemented in Rust for interop.
    The only real change is that it doesn't accept None as a value, and methods are modified to reflect that"""

    @classmethod
    def fromkeys(self, iterable: Iterable[str], value: str) -> 'DynamicDict':
        ...
    
    def __contains__(self, item: str) -> bool:
        """Implement 'key in DynamicDict'"""
        ...

    def __delitem__(self, key):
        """Implement del"""
        ...

    def __iter__(self) -> Iterator[Tuple[str,str]]:
        ...
    
    def __eq__(self, other: Any) -> bool:
        """Implement DynamicDict == object"""
        ...

    def __getitem__(self, key: str) -> str:
        """Implement '[]'"""
        ...

    def __len__(self) -> int:
        """Implement len(DynamicDict)"""
        ...

    def __lt__(self, other: 'DynamicDict') -> bool:
        """Implement '<' '>' for two DynamicDicts"""
        ...

    def __setitem__(self, key: str, item: str):
        """Implement '[]='"""
        ...

    def __str__(self) -> str:
        """Implement str(DynamicDict)"""
        ...

    def __repr__(self) -> str:
        ...
    
    def __reversed__(self) -> Iterator[Tuple[str,str]]:
        ...

    def clear(self):
        ...

    def copy(self) -> 'DynamicDict':
        ...

    def get(self, key: str) -> str:
        ...

    def has_key(self, k: str) -> bool:
        ...

    def items(self) -> 'DynamicDictItemsView':
        ...
    
    def keys(self) -> 'DynamicDictKeysView':
        ...
    
    def pop(self, key: str, default: Optional[str]) -> str:
        ...

    def popitem(self) -> Tuple[str, str]:
        ...

    def setdefault(self, key: str, default: str) -> str:
        ...

    def update(self, *args, **kwargs):
        ...

    def values(self) -> 'DynamicDictValuesView':
        ...

class DynamicDictItemsView(Iterable):
    def __contains__(self, item: str) -> bool:
        """Implement 'key in DynamicDictItemsView'"""
        ...

    def __iter__(self) -> Iterator[Tuple[str,str]]:
        ...

    def __len__(self) -> int:
        """Implement len(DynamicDictItemsView)"""
        ...

    def __reversed__(self) -> Iterator[Tuple[str,str]]:
        ...

class DynamicDictKeysView(Iterable):
    def __contains__(self, item: str) -> bool:
        """Implement 'key in DynamicDictKeysView'"""
        ...

    def __iter__(self) -> Iterator[str]:
        ...

    def __len__(self) -> int:
        """Implement len(DynamicDictKeysView)"""
        ...

    def __reversed__(self) -> Iterator[str]:
        ...

class DynamicDictValuesView(Iterable):
    def __contains__(self, item: str) -> bool:
        """Implement 'key in DynamicDictValuesView'"""
        ...

    def __iter__(self) -> Iterator[str]:
        ...

    def __len__(self) -> int:
        """Implement len(DynamicDictValuesView)"""
        ...

    def __reversed__(self) -> Iterator[str]:
        ...

class SatelliteData:
    """Data referring to a satellite"""
    uuid: str

class IntentData:
    """Data referring to an intent"""
    input: str
    name: str
    confidence: float
    slots: Dict[str, str]

class ActionContext:
    """Represents how an action is called, e.g: Input made by user, confidence,
    slots ,if it was en event..."""

    locale: str
    satellite: Optional[SatelliteData]
    data: Union[str,IntentData]

class ActionAnswer():
    @staticmethod
    def load_audio(path: str, end_session: bool = True)-> 'ActionAnswer':
        ...

    @staticmethod
    def text(text: str, end_session: bool = True) -> 'ActionAnswer':
        ...