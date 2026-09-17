"""Attack definitions — the single place to edit what gets sent.

Every payload the bot can send is defined here as a plain, writable
:class:`~bot.game_data.attack.Attack` object built from the in-house
``side()`` / ``wave()`` helpers. Nothing in here talks to the network, the
database or the scheduler; it is pure data.

To add or change an attack:
    1. compose it with ``Attack(wave1=wave(left=side(...)))`` below
    2. reference it from :mod:`bot.tasks`
    3. optionally add a readable name to ``ATTACK_REGISTRY`` for the CLI

Use the ``troops`` / ``tools`` catalogues for ids::

    python -m bot.cli troops
"""

from __future__ import annotations

from . import berimond
from .game_data import Attack, Tool, Troop, side, wave

# ---------------------------------------------------------------------------
# Sand kingdom (Burning Sands RBC)
# ---------------------------------------------------------------------------

#: Level-61 RBC clear. 50 crossbowmen on the left flank, no tools.
#: This is the only payload that is valid once ADI confirms ``TL == 61``.
SANDS_LV61 = Attack(
    wave1=wave(
        left=side(units=[(Troop.CROSSBOWMAN, 50)]),
    )
)

#: Level 36-60 RBC clear using a mead-buffed flank.
SANDS_LV36_60_MEAD = Attack(
    wave1=wave(
        left=side(units=[(Troop.VALKYRIE_RANGER_10, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
        right=side(units=[(Troop.VALKYRIE_RANGER_10, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
    ),
    wave2=wave(
        left=side(units=[(Troop.VALKYRIE_RANGER_10, 30)]),
        right=side(units=[(Troop.VALKYRIE_RANGER_10, 30)]),
    ),
    wave3=wave(
        left=side(units=[(Troop.VALKYRIE_RANGER_10, 30)]),
        right=side(units=[(Troop.VALKYRIE_RANGER_10, 30)]),
    ),
    wave4=wave(
        left=side(units=[(Troop.VALKYRIE_RANGER_10, 30)]),
        right=side(units=[(Troop.VALKYRIE_RANGER_10, 30)]),
    ),
)

#: Sub-61 RBC clear (ladders + relic shortbowmen), 4 identical waves.
SANDS_BELOW_61 = Attack(
    wave1=wave(
        left=side(units=[(Troop.RELIC_SHORTBOWMAN_0, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
        right=side(units=[(Troop.RELIC_SHORTBOWMAN_0, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
    ),
    wave2=wave(
        left=side(units=[(Troop.RELIC_SHORTBOWMAN_0, 30)]),
        right=side(units=[(Troop.RELIC_SHORTBOWMAN_0, 30)]),
    ),
    wave3=wave(
        left=side(units=[(Troop.RELIC_SHORTBOWMAN_0, 30)]),
        right=side(units=[(Troop.RELIC_SHORTBOWMAN_0, 30)]),
    ),
    wave4=wave(
        left=side(units=[(Troop.RELIC_SHORTBOWMAN_0, 30)]),
        right=side(units=[(Troop.RELIC_SHORTBOWMAN_0, 30)]),
    ),
)

#: Levels 35-61 RBC clear using deathly horrors, 4 identical waves.
#:
#: 5 ladders per 30 troops is the most the game accepts per wave (5/30 = 17%).
#: 7 ladders per 30 troops was rejected with status 5, "the action could not be
#: performed", so do not raise this without evidence from a real capture.
SANDS_35_61_DEATHLY_HORROR = Attack(
    wave1=wave(
        left=side(units=[(Troop.DEATHLY_HORROR, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
        right=side(units=[(Troop.DEATHLY_HORROR, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
    ),
    wave2=wave(
        left=side(units=[(Troop.DEATHLY_HORROR, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
        right=side(units=[(Troop.DEATHLY_HORROR, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
    ),
    wave3=wave(
        left=side(units=[(Troop.DEATHLY_HORROR, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
        right=side(units=[(Troop.DEATHLY_HORROR, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
    ),
    wave4=wave(
        left=side(units=[(Troop.DEATHLY_HORROR, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
        right=side(units=[(Troop.DEATHLY_HORROR, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
    ),
)

# ---------------------------------------------------------------------------
# Storm kingdom
# ---------------------------------------------------------------------------

#: Storm event target. Placeholder composition — adjust before enabling.
STORM_DEMON_HORROR = Attack(
    wave1=wave(
        left=side(units=[(Troop.DEMON_HORROR, 50)]),
        right=side(units=[(Troop.DEMON_HORROR, 50)]),
    ),
    wave2=wave(
        left=side(units=[(Troop.DEMON_HORROR, 50)]),
        right=side(units=[(Troop.DEMON_HORROR, 50)]),
    ),
    wave3=wave(
        left=side(units=[(Troop.DEMON_HORROR, 50)]),
        right=side(units=[(Troop.DEMON_HORROR, 50)]),
    ),
    wave4=wave(
        left=side(units=[(Troop.DEMON_HORROR, 50)]),
        right=side(units=[(Troop.DEMON_HORROR, 50)]),
    ),
)

# ---------------------------------------------------------------------------
# Berimond
# ---------------------------------------------------------------------------

#: Berimond event attack. The composition lives in :mod:`bot.berimond` because
#: it is paired with the Berimond source/target coordinates.
BERIMOND_FIXED = berimond.ATTACK

# ---------------------------------------------------------------------------
# Registry (used by `python -m bot.cli attacks`)
# ---------------------------------------------------------------------------
#
# Built automatically from every module-level Attack object above, so adding an
# attack needs no extra bookkeeping. The key is the variable name lowercased:
# SANDS_LV61 -> "sands_lv61".


def _collect_attacks() -> dict[str, Attack]:
    found: dict[str, Attack] = {}
    for name, value in list(globals().items()):
        if name.startswith("_") or not isinstance(value, Attack):
            continue
        found[name.lower()] = value
    return found


ATTACK_REGISTRY: dict[str, Attack] = _collect_attacks()

__all__ = [
    "ATTACK_REGISTRY",
    "BERIMOND_FIXED",
    "SANDS_35_61_DEATHLY_HORROR",
    "SANDS_BELOW_61",
    "SANDS_LV36_60_MEAD",
    "SANDS_LV61",
    "STORM_DEMON_HORROR",
]
