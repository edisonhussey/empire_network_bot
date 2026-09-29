"""Editable Storm Islands attack configuration."""

from __future__ import annotations

from ...game_data import KINGDOM, Attack, Troop, side, wave
from ...scheduler import task_definition


MAX_CONSECUTIVE_CRA_REJECTS = 2
SCAN_INTERVAL_RANGE = (5.5, 12.5)
SCAN_RADIUS = 96
TARGET_FRESH_SECONDS = 120
# The owned Storm castle is account/session state.  The proxy listener learns it
# from the login ``gbd`` packet; these ``None`` values mean "auto-discover".
# ``--source-x`` and ``--source-y`` remain available as an explicit emergency
# override, but an attack must never silently use another account's coordinates.
SOURCE_X = None
SOURCE_Y = None
HBW = -1
PTT = 1


# Four-wave Storm army: 50 kunai throwers on each flank per wave.
# 2 flanks × 50 troops × 4 waves = 400 troops total.
_KUNAI_FLANK_WAVE = wave(
    left=side(units=[(Troop.RENEGADE_KUNAI_THROWER, 50)]),
    right=side(units=[(Troop.RENEGADE_KUNAI_THROWER, 50)]),
)
STORM_RENEGADE_KUNAI_FLANKS = Attack(
    wave1=_KUNAI_FLANK_WAVE,
    wave2=_KUNAI_FLANK_WAVE,
    wave3=_KUNAI_FLANK_WAVE,
    wave4=_KUNAI_FLANK_WAVE,
)


# Explicit Storm mode uses this definition directly. It is intentionally not
# added to bot/tasks.py account subscriptions, so enabling Storm does not make
# `--mode auto` switch an account away from its normal Sands plan.
STORM = task_definition(
    "storm",
    kingdom=KINGDOM.storm,
    target_levels=(60, 70, 80),
    commanders=14,
    priority=20,
    enabled=True,
    attack=STORM_RENEGADE_KUNAI_FLANKS,
    tags=("storm",),
    notes="Storm event targets using the mode-local attack and scan policy.",
)

TASK_DEFINITIONS = (STORM,)
TASKS_BY_NAME = {definition.name: definition for definition in TASK_DEFINITIONS}


__all__ = [
    "HBW",
    "MAX_CONSECUTIVE_CRA_REJECTS",
    "PTT",
    "SCAN_INTERVAL_RANGE",
    "SCAN_RADIUS",
    "SOURCE_X",
    "SOURCE_Y",
    "STORM_RENEGADE_KUNAI_FLANKS",
    "STORM",
    "TASK_DEFINITIONS",
    "TASKS_BY_NAME",
    "TARGET_FRESH_SECONDS",
]
