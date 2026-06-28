from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any


if __package__ in {None, ""}:
    repo_root = Path(__file__).resolve().parents[2]
    sys.path.insert(0, str(repo_root))
    from empire.attacks.troops import Troop
    from empire.attacks.tools import Tool
else:
    from .troops import Troop
    from .tools import Tool


EMPTY_SLOT = [-1, 0]
SIDE_KEYS = {
    "left": "L",
    "l": "L",
    "middle": "M",
    "mid": "M",
    "m": "M",
    "right": "R",
    "r": "R",
}
SIDE_SLOTS = {
    "L": {"tools": 2, "units": 2},
    "M": {"tools": 3, "units": 6},
    "R": {"tools": 2, "units": 2},
}


def slot(item: Troop | Tool | int, amount: int) -> list[int]:
    if amount <= 0:
        raise ValueError("amount must be greater than 0")
    item_id = item.id if hasattr(item, "id") else int(item)
    return [item_id, amount]


def padded_slots(items: list[list[int]] | None, total: int) -> list[list[int]]:
    items = items or []
    if len(items) > total:
        raise ValueError(f"too many items for {total} slots")
    return [*items, *[EMPTY_SLOT.copy() for _ in range(total - len(items))]]


def normalize_items(items: list[tuple[Troop | Tool | int, int]] | None) -> list[list[int]]:
    return [slot(item, amount) for item, amount in (items or [])]


def side(
    *,
    tools: list[tuple[Tool | int, int]] | None = None,
    units: list[tuple[Troop | int, int]] | None = None,
) -> dict[str, list[tuple[Any, int]]]:
    return {
        "tools": tools or [],
        "units": units or [],
    }


def wave(
    *,
    left: dict[str, Any] | None = None,
    middle: dict[str, Any] | None = None,
    right: dict[str, Any] | None = None,
) -> dict[str, dict[str, Any]]:
    config: dict[str, dict[str, Any]] = {}
    if left:
        config["left"] = left
    if middle:
        config["middle"] = middle
    if right:
        config["right"] = right
    return config


class Attack:
    def __init__(
        self,
        *,
        wave1: dict[str, Any] | None = None,
        wave2: dict[str, Any] | None = None,
        wave3: dict[str, Any] | None = None,
        wave4: dict[str, Any] | None = None,
    ) -> None:
        self.wave1 = wave1 or {}
        self.wave2 = wave2
        self.wave3 = wave3
        self.wave4 = wave4

    def to_payload(self) -> list[dict[str, dict[str, list[list[int]]]]]:
        waves = [self.wave1, self.wave2, self.wave3, self.wave4]
        return [self._wave_payload(config) for config in waves if config is not None]

    def to_json(self) -> str:
        return json.dumps(self.to_payload(), separators=(",", ":"))

    def to_attack_part(self) -> dict[str, list[dict[str, dict[str, list[list[int]]]]]]:
        return {"A": self.to_payload()}

    def _wave_payload(self, config: dict[str, Any]) -> dict[str, dict[str, list[list[int]]]]:
        payload = self._empty_wave()
        for side_name, side_config in config.items():
            side_key = SIDE_KEYS[side_name]
            slots = SIDE_SLOTS[side_key]
            payload[side_key] = {
                "T": padded_slots(normalize_items(side_config.get("tools")), slots["tools"]),
                "U": padded_slots(normalize_items(side_config.get("units")), slots["units"]),
            }
        return payload

    @staticmethod
    def _empty_wave() -> dict[str, dict[str, list[list[int]]]]:
        return {
            key: {
                "T": padded_slots([], slots["tools"]),
                "U": padded_slots([], slots["units"]),
            }
            for key, slots in SIDE_SLOTS.items()
        }


if __name__ == "__main__":
    attack = Attack(
        wave1=wave(
            middle=side(
                tools=[(Tool.SCALING_LADDER, 10)],
                units=[(Troop.VALKYRIE_RANGER_0, 28)],
            )
        ),
        wave2 = wave(
            left=side(
                tools=[(Tool.SCALING_LADDER, 10)],
                units=[(Troop.VALKYRIE_RANGER_0, 5)],
            )
        )
    )

    print(json.dumps(attack.to_attack_part(), indent=2))
