"""Editable Burning Sands attacks and task definitions."""

from __future__ import annotations

from ...game_data import KINGDOM, Attack, Tool, Troop, side, wave
from ...scheduler import task_definition


SANDS_LV61 = Attack(
    wave1=wave(left=side(units=[(Troop.CROSSBOWMAN, 50)]))
)

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

# Five ladders per 30 troops is the highest measured accepted ratio. Seven was
# rejected with status 5, so do not raise it without a confirmed capture.
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

SANDS_35_61_KUNAI = Attack(
    wave1=wave(
        left=side(units=[(Troop.RENEGADE_KUNAI_THROWER, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
        right=side(units=[(Troop.RENEGADE_KUNAI_THROWER, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
    ),
    wave2=wave(
        left=side(units=[(Troop.RENEGADE_KUNAI_THROWER, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
        right=side(units=[(Troop.RENEGADE_KUNAI_THROWER, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
    ),
    wave3=wave(
        left=side(units=[(Troop.RENEGADE_KUNAI_THROWER, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
        right=side(units=[(Troop.RENEGADE_KUNAI_THROWER, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
    ),
    wave4=wave(
        left=side(units=[(Troop.RENEGADE_KUNAI_THROWER, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
        right=side(units=[(Troop.RENEGADE_KUNAI_THROWER, 30)], tools=[(Tool.SCALING_LADDER, 5)]),
    ),
)


SAND_LV61_CROSSBOW = task_definition(
    "sand_rbc_level_61_crossbow",
    kingdom=KINGDOM.sand,
    target_level=61,
    commanders=16,
    priority=20,
    attack=SANDS_LV61,
    tags=("rbc", "sand"),
    notes="50 crossbowmen on left flank. Uses live ADI/CRA/GAM/CAT state before sending.",
)

SAND_LV61_CROSSBOW_20C = task_definition(
    "sand_rbc_level_61_crossbow_20c",
    kingdom=KINGDOM.sand,
    target_level=61,
    commanders=20,
    priority=30,
    attack=SANDS_LV61,
    tags=("rbc", "sand"),
    notes="As sand_rbc_level_61_crossbow but 20 commanders at higher priority.",
)

SAND_LV36_60_MEAD_FLANK = task_definition(
    "sand_rbc_level_36_60_mead_flank",
    kingdom=KINGDOM.sand,
    target_levels=tuple(range(35, 61)),
    commanders=19,
    priority=10,
    enabled=True,
    attack=SANDS_LV36_60_MEAD,
    tags=("rbc", "sand", "mead"),
    notes="Mead flank attack for Sands RBC levels 36-60 using commanders 17-35.",
)

SAND_KUNAI = task_definition(
    "sand_kunai",
    kingdom=KINGDOM.sand,
    target_levels=tuple(range(35, 61)),
    commanders=19,
    priority=10,
    enabled=True,
    attack=SANDS_35_61_KUNAI,
    tags=("rbc", "sand", "kunai"),
    notes="Renegade kunai flank attack for Sands RBC levels 35-60.",
)

SAND_LV35_61_DEATHLY_HORROR = task_definition(
    "sand_lv35_61_dh",
    kingdom=KINGDOM.sand,
    target_levels=tuple(range(35, 61)),
    commanders=18,
    priority=10,
    enabled=True,
    attack=SANDS_35_61_DEATHLY_HORROR,
    tags=("rbc", "sand"),
    notes="Deathly horror clear for Sands RBC levels 35-60, 30 + 5 ladders per side.",
)


__all__ = [
    "SAND_KUNAI",
    "SAND_LV35_61_DEATHLY_HORROR",
    "SAND_LV36_60_MEAD_FLANK",
    "SAND_LV61_CROSSBOW",
    "SAND_LV61_CROSSBOW_20C",
    "SANDS_35_61_KUNAI",
    "SANDS_35_61_DEATHLY_HORROR",
    "SANDS_BELOW_61",
    "SANDS_LV36_60_MEAD",
    "SANDS_LV61",
]
