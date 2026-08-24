from __future__ import annotations

from types import SimpleNamespace

from .attack import Attack, side, wave
from .kingdom import Kingdom
from .tools import Tool
from .troops import Troop


def _enum_aliases(enum_type):
    return SimpleNamespace(**{member.name.lower(): member for member in enum_type})


KINGDOM = SimpleNamespace(
    green=Kingdom.GREEN_KINGDOM,
    sand=Kingdom.SAND_KINGDOM,
    ice=Kingdom.ICE_KINGDOM,
    fire=Kingdom.FIRE_KINGDOM,
    storm=Kingdom.STORM_KINGDOM,
    berimond=Kingdom.BERIMOND_KINGDOM,
)
TROOP = _enum_aliases(Troop)
TOOL = _enum_aliases(Tool)

__all__ = [
    "Attack",
    "KINGDOM",
    "Kingdom",
    "TOOL",
    "TROOP",
    "Tool",
    "Troop",
    "side",
    "wave",
]
