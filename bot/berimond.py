from __future__ import annotations

from .game_data import Attack, Tool, Troop, side, wave


KID = 10
SOURCE_X = 1310
SOURCE_Y = 111
TARGET_X = 1097
TARGET_Y = 68
HBW = -1
PTT = 1
AV = 1

COMMANDER_COUNT = 10
GLOBAL_ATTACK_COOLDOWN_SECONDS = 4.0
MAX_CRA_ERRORS = 2
CRA_ERROR_WINDOW_SECONDS = 10 * 60
ADI_TO_CRA_TARGET_SECONDS = 3.0
MAX_COMMANDER_OUT_SECONDS = 6 * 60
ALERT_ON_ERROR = True
ALERT_SOUND_PATH = "/System/Library/Sounds/Sosumi.aiff"

ATTACK = Attack(
    wave1=wave(
        left=side(
            units=[(Troop.MARKSMAN, 8), (Troop.VETERAN_DEATHLY_HORROR, 18)],
            tools=[(Tool.MANTLET, 30)],
        )
    )
)
