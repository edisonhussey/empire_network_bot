from __future__ import annotations

from .game_data import Attack, Tool, Troop, side, wave


KID = 10
SOURCE_X = 1310
SOURCE_Y = 111
TARGET_X = 1139
TARGET_Y = 95
HBW = -1
PTT = 1
AV = 1

COMMANDER_COUNT = 6
GLOBAL_ATTACK_COOLDOWN_SECONDS = 4.0
MAX_CRA_ERRORS = 2
CRA_ERROR_WINDOW_SECONDS = 10 * 60
ADI_TO_CRA_TARGET_SECONDS = 3.0
MAX_COMMANDER_OUT_SECONDS = 6 * 60
ALERT_ON_ERROR = True
ALERT_SOUND_PATH = "/System/Library/Sounds/Sosumi.aiff"

#: How many troops the Berimond camp can hold. 302 for veteran deathly horror
#: (unit 10) - the camp reads 299 when it is as full as it gets. Used to size a
#: refill transfer: send ``0.9`` of what is missing, never the whole capacity.
TOTAL_CAPACITY = 302
CAPACITY_BY_UNIT = {10: TOTAL_CAPACITY}

ATTACK = Attack(
    wave1=wave(
        left=side(
            units=[(Troop.MARKSMAN, 12), (Troop.VETERAN_DEATHLY_HORROR, 14)],
            tools=[(Tool.MANTLET, 30)],
        )
    )
)
