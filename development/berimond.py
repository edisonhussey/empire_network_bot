from __future__ import annotations

import argparse
import json
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Iterable


BERIMOND_KID = 10
BERIMOND_AREA_TYPE = 10

# Observed in GAA rows:
# [10, x, y, object_id, owner_id, resource_type, size/tier, cooldown_or_flag, name]
RESOURCE_TYPES = {
    0: "wood",
    1: "stone",
    2: "food",
}


@dataclass(frozen=True)
class BerimondVillage:
    x: int
    y: int
    object_id: int
    owner_id: int
    resource_type: int | None
    tier: int | None
    name: str | None
    source_command: str
    raw: list[Any]

    @property
    def resource_name(self) -> str | None:
        if self.resource_type is None:
            return None
        return RESOURCE_TYPES.get(self.resource_type, f"unknown_{self.resource_type}")


@dataclass(frozen=True)
class BerimondCamp:
    x: int
    y: int
    object_id: int
    owner_id: int
    name: str | None
    raw: list[Any]


@dataclass
class BerimondCycleState:
    active: bool = False
    first_seen_at: int | None = None
    last_seen_at: int | None = None
    first_missing_at: int | None = None
    last_missing_at: int | None = None
    observed_active_count: int = 0
    observed_missing_count: int = 0
    expected_return_after: int | None = None
    notes: str = ""

    def observe(self, *, seen: bool, observed_at: int | None = None, expected_cycle_seconds: int | None = None) -> None:
        epoch = int(time.time() if observed_at is None else observed_at)
        if seen:
            if self.first_seen_at is None:
                self.first_seen_at = epoch
            self.last_seen_at = epoch
            self.active = True
            self.observed_active_count += 1
            return

        if self.first_missing_at is None or self.active:
            self.first_missing_at = epoch
        self.last_missing_at = epoch
        self.active = False
        self.observed_missing_count += 1
        if expected_cycle_seconds and self.last_seen_at:
            self.expected_return_after = self.last_seen_at + int(expected_cycle_seconds)


def int_or_none(value: Any) -> int | None:
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def parse_xt_packet(packet: str) -> dict[str, Any] | None:
    if not packet.startswith("%xt%"):
        return None
    fields = packet.strip().strip("%").split("%")
    if len(fields) < 5 or fields[0] != "xt":
        return None

    if fields[1].startswith("EmpireEx_"):
        command = fields[2]
        request_id = fields[3]
    else:
        command = fields[1]
        request_id = fields[2]

    try:
        payload = json.loads(fields[-1])
    except json.JSONDecodeError:
        return None
    return {"command": command, "request_id": request_id, "payload": payload}


def extract_raw_packet(block: str) -> str | None:
    for line in reversed(block.splitlines()):
        line = line.strip()
        if line.startswith("%xt%"):
            return line
    return None


def iter_log_packets(paths: Iterable[Path]) -> Iterable[tuple[Path, dict[str, Any]]]:
    for path in paths:
        text = path.read_text(encoding="utf-8", errors="replace")
        for block in text.split("---"):
            packet = extract_raw_packet(block)
            if not packet:
                continue
            parsed = parse_xt_packet(packet)
            if parsed is not None:
                yield path, parsed


def village_from_area_row(row: list[Any], source_command: str) -> BerimondVillage | None:
    if len(row) < 5 or int_or_none(row[0]) != BERIMOND_AREA_TYPE:
        return None
    x = int_or_none(row[1])
    y = int_or_none(row[2])
    object_id = int_or_none(row[3])
    owner_id = int_or_none(row[4])
    if x is None or y is None or object_id is None or owner_id is None:
        return None
    name = row[8] if len(row) > 8 and isinstance(row[8], str) else None
    return BerimondVillage(
        x=x,
        y=y,
        object_id=object_id,
        owner_id=owner_id,
        resource_type=int_or_none(row[5]) if len(row) > 5 else None,
        tier=int_or_none(row[6]) if len(row) > 6 else None,
        name=name,
        source_command=source_command,
        raw=row,
    )


def village_from_vp_row(row: list[Any], source_command: str) -> BerimondVillage | None:
    if len(row) < 5 or int_or_none(row[4]) != BERIMOND_KID:
        return None
    resource_type = int_or_none(row[0])
    object_id = int_or_none(row[1])
    x = int_or_none(row[2])
    y = int_or_none(row[3])
    if object_id is None or x is None or y is None:
        return None
    return BerimondVillage(
        x=x,
        y=y,
        object_id=object_id,
        owner_id=-1,
        resource_type=resource_type,
        tier=None,
        name=None,
        source_command=source_command,
        raw=row,
    )


def camp_from_gcl_ai(row: list[Any]) -> BerimondCamp | None:
    if len(row) < 5:
        return None
    x = int_or_none(row[1])
    y = int_or_none(row[2])
    object_id = int_or_none(row[3])
    owner_id = int_or_none(row[4])
    if x is None or y is None or object_id is None or owner_id is None:
        return None
    name = row[10] if len(row) > 10 and isinstance(row[10], str) else None
    return BerimondCamp(x=x, y=y, object_id=object_id, owner_id=owner_id, name=name, raw=row)


def berimond_from_payload(command: str, payload: dict[str, Any]) -> tuple[list[BerimondVillage], list[BerimondCamp]]:
    villages: list[BerimondVillage] = []
    camps: list[BerimondCamp] = []

    if command == "gaa":
        for row in payload.get("AI") or []:
            if isinstance(row, list):
                village = village_from_area_row(row, command)
                if village is not None:
                    villages.append(village)

    objects = payload.get("O")
    if isinstance(objects, dict):
        objects = [objects]
    if isinstance(objects, list):
        for owner in objects:
            if not isinstance(owner, dict):
                continue
            for row in owner.get("VP") or []:
                if isinstance(row, list):
                    village = village_from_vp_row(row, command)
                    if village is not None:
                        villages.append(village)

    gcl = payload.get("gcl")
    if isinstance(gcl, dict):
        for group in gcl.get("C") or []:
            if not isinstance(group, dict) or int_or_none(group.get("KID")) != BERIMOND_KID:
                continue
            for item in group.get("AI") or []:
                row = item.get("AI") if isinstance(item, dict) else None
                if isinstance(row, list):
                    camp = camp_from_gcl_ai(row)
                    if camp is not None:
                        camps.append(camp)

    kgv = payload.get("kgv")
    if isinstance(kgv, dict):
        for wrapper in kgv.get("VI") or []:
            rows = wrapper if isinstance(wrapper, list) else []
            for row in rows:
                if isinstance(row, list):
                    village = village_from_area_row(row, command)
                    if village is not None:
                        villages.append(village)

    return villages, camps


def summarize_logs(paths: Iterable[Path]) -> dict[str, Any]:
    village_by_id: dict[int, BerimondVillage] = {}
    camps_by_id: dict[int, BerimondCamp] = {}
    commands: dict[str, int] = {}

    for _path, parsed in iter_log_packets(paths):
        command = str(parsed["command"])
        payload = parsed["payload"]
        if not isinstance(payload, dict):
            continue
        found_villages, found_camps = berimond_from_payload(command, payload)
        if found_villages or found_camps:
            commands[command] = commands.get(command, 0) + 1
        for village in found_villages:
            village_by_id[village.object_id] = village
        for camp in found_camps:
            camps_by_id[camp.object_id] = camp

    resources: dict[str, int] = {}
    for village in village_by_id.values():
        key = village.resource_name or "unknown"
        resources[key] = resources.get(key, 0) + 1

    return {
        "berimond_kid": BERIMOND_KID,
        "commands_with_berimond_data": commands,
        "village_count": len(village_by_id),
        "camp_count": len(camps_by_id),
        "resources": resources,
        "sample_villages": [asdict(item) for item in list(village_by_id.values())[:10]],
        "sample_camps": [asdict(item) for item in list(camps_by_id.values())[:10]],
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Summarize Berimond-related data from recorded GGE logs.")
    parser.add_argument("logs", nargs="*", type=Path, help="Log files to parse. Defaults to bot/logs/gge_20260902_*.log.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    paths = args.logs or sorted(Path("bot/logs").glob("gge_20260902_*.log"))
    print(json.dumps(summarize_logs(paths), indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
