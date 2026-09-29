"""Compatibility catalogue aggregating each mode's local attack config.

Edit attacks beside their mode under :mod:`bot.event`; this module only keeps
the existing CLI and imports stable.
"""

from __future__ import annotations

from .event.berimond_kingdom.config import ATTACK as BERIMOND_FIXED
from .event.sand.config import (
    SANDS_35_61_DEATHLY_HORROR,
    SANDS_BELOW_61,
    SANDS_LV36_60_MEAD,
    SANDS_LV61,
)
from .event.storm.config import STORM_RENEGADE_KUNAI_FLANKS
from .game_data import Attack


def _collect_attacks() -> dict[str, Attack]:
    return {
        name.lower(): value
        for name, value in globals().items()
        if not name.startswith("_") and isinstance(value, Attack)
    }


ATTACK_REGISTRY: dict[str, Attack] = _collect_attacks()

__all__ = [
    "ATTACK_REGISTRY",
    "BERIMOND_FIXED",
    "SANDS_35_61_DEATHLY_HORROR",
    "SANDS_BELOW_61",
    "SANDS_LV36_60_MEAD",
    "SANDS_LV61",
    "STORM_RENEGADE_KUNAI_FLANKS",
]
