from __future__ import annotations

import argparse
import json
import math
import random
import re
import sys
import time
from datetime import datetime
from pathlib import Path
from typing import Any


if __package__ in {None, ""}:
    REPO_ROOT = Path(__file__).resolve().parents[2]
    sys.path.insert(0, str(REPO_ROOT))
else:
    REPO_ROOT = Path(__file__).resolve().parents[2]

from empire.attacks.attack import Attack, side, wave
from empire.attacks.troops import Troop
from empire.game_data.tools import Tool

HERE = Path(__file__).resolve().parent
COMMANDER_STATE_PATH = HERE / "commander_state.json"
RBC_STATE_PATH = HERE / "rbc_state.json"
LATEST_SERVER_MESSAGES = HERE / "latest_server_messages"
PROXY_SEND_FILE = REPO_ROOT / "captures" / "inject3_send.txt"
CAPTURE_LOGS_DIR = REPO_ROOT / "captures"

SAND_SERVER_HEADER = "EmpireEx_21"
KINGDOM_ID = 1
SOURCE_X = 593
SOURCE_Y = 613

HBW_VALUE = 1007  # coin travel
REQUEST_COOLDOWN_SECONDS = 10.4
STRICT_ATTACK_PACKET_COOLDOWN_MIN = 12.3
ADI_TO_ATTACK_DELAY_RANGE = (4.0, 7.0)
ADI_REFRESH_AFTER_SECONDS = 45 * 60 + 17
ADI_REFRESH_PROBABILITY = 0.12
UNKNOWN_ADI_RETRY_RANGE = (52.7, 98.4)
NO_LV61_LID_RETRY_RANGE = (30 * 60, 40 * 60)
BACKUP_LID_RETURN_SECONDS = 50 * 60
FALLBACK_RETURN_TRAVEL_MULTIPLIER = 1.4
RETURN_RANDOM_HOLD_RANGE = (10.4, 29.7)
MIN_REAL_EPOCH = 1_700_000_000
PENDING_CRA_TIMEOUT_RANGE = (116.5, 184.2)
CRA_ERROR_TARGET_BACKOFF_RANGE = (12 * 60 + 7.0, 31 * 60 + 43.0)
TARGET_FAIL_BACKOFF_BASE_SECONDS = 10 * 60
TARGET_FAIL_BACKOFF_MAX_SECONDS = 6 * 60 * 60
TARGET_FAIL_BACKOFF_JITTER_RANGE = (0.88, 1.27)
MAP_SCAN_RADIUS = 50
MAX_TARGET_DISTANCE = 100.0
MAP_CHUNK_SIZE = 13
SETUP_SCAN_DELAY_RANGE = (10.4, 14.8)
REFRESH_SCAN_DELAY_RANGE = (12 * 60, 28 * 60)
AREA_BARRON = 2
REQUEST_SEQUENCE_TIMEOUT_MINUTES = 0  # 0 means no automatic stop.
AUTO_REFRESH_SCAN = False

VALID_LIDS = (0, *range(2, 36))
SHIELD_MAIDEN_LIDS = (0, *range(2, 11))
LV61_LIDS = SHIELD_MAIDEN_LIDS
NON_LV61_LIDS = tuple(lid for lid in VALID_LIDS if lid not in SHIELD_MAIDEN_LIDS)
LV61 = 61


# strict sequence is ADI -> 4-7 seconds loose/random -> cra packet -> track MID/LID.
# If ADI says this is not visible level 61, never use the level 61 attack.
LEVEL_61 = Attack(
    wave1=wave(
        left=side(
            tools=[],
            units=[(Troop.CROSSBOWMAN, 50)],
        )
    )
)

NOT_LV_61 = Attack(
    wave1=wave(
        left=side(tools=[(Tool.SCALING_LADDER, 7)], units=[(Troop.VALKYRIE_RANGER_10, 50)]),
        right=side(tools=[(Tool.SCALING_LADDER, 7)], units=[(Troop.VALKYRIE_RANGER_10, 50)]),
    ),
    wave2=wave(
        left=side(tools=[], units=[(Troop.VALKYRIE_RANGER_10, 50)]),
        right=side(tools=[], units=[(Troop.VALKYRIE_RANGER_10, 50)]),
    ),
    wave3=wave(
        left=side(tools=[], units=[(Troop.VALKYRIE_RANGER_10, 50)]),
        right=side(tools=[], units=[(Troop.VALKYRIE_RANGER_10, 50)]),
    ),
    wave4=wave(
        left=side(tools=[], units=[(Troop.VALKYRIE_RANGER_10, 50)]),
        right=side(tools=[], units=[(Troop.VALKYRIE_RANGER_10, 50)]),
    ),
)


NOT_LV_50_AND_BELOW = Attack(
    wave1=wave(
        left=side(tools=[(Tool.SCALING_LADDER, 7)], units=[(Troop.VALKYRIE_RANGER_10, 30)]),
        right=side(tools=[(Tool.SCALING_LADDER, 7)], units=[(Troop.VALKYRIE_RANGER_10, 30)]),
    ),
    wave2=wave(
        left=side(tools=[], units=[(Troop.VALKYRIE_RANGER_10, 30)]),
        right=side(tools=[], units=[(Troop.VALKYRIE_RANGER_10, 30)]),
    ),
    wave3=wave(
        left=side(tools=[], units=[(Troop.VALKYRIE_RANGER_10, 30)]),
        right=side(tools=[], units=[(Troop.VALKYRIE_RANGER_10, 30)]),
    ),
    wave4=wave(
        left=side(tools=[], units=[(Troop.VALKYRIE_RANGER_10, 30)]),
        right=side(tools=[], units=[(Troop.VALKYRIE_RANGER_10, 30)]),
    ),
)


def rbc_cooldown_addition() -> int:
    return 3600 * 3 + 151 - random.randrange(-48, 81)


def sands_level_from_gaa_value(gaa_value: int | None) -> int | None:
    if gaa_value is None:
        return None
    return math.floor(1.9 * math.pow(max(0, int(gaa_value)), 0.555)) + 35


def now_epoch() -> float:
    return time.time()


def random_between(seconds_range: tuple[float, float]) -> float:
    return random.uniform(seconds_range[0], seconds_range[1])


def log(message: str) -> None:
    print(time.strftime("[%H:%M:%S]"), message, flush=True)


def load_json(path: Path, default: dict[str, Any]) -> dict[str, Any]:
    if not path.exists():
        return default.copy()
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return default.copy()
    return data if isinstance(data, dict) else default.copy()


def clean_epoch(value: Any) -> float:
    epoch = float(value or 0.0)
    if 0 < epoch < MIN_REAL_EPOCH:
        return 0.0
    return epoch


def save_json(path: Path, data: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    tmp.replace(path)


def commander_default(lid: int) -> dict[str, Any]:
    return {
        "epoch_available": 0.0,
        "in_use": False,
        "shield_maiden": lid in SHIELD_MAIDEN_LIDS,
        "status": "available",
        "MID": None,
        "target": None,
        "backup_return": False,
        "last_update": 0.0,
    }


def load_commander_state() -> dict[str, Any]:
    state = load_json(COMMANDER_STATE_PATH, {})
    for lid in VALID_LIDS:
        key = str(lid)
        current = state.get(key)
        if not isinstance(current, dict):
            state[key] = commander_default(lid)
            continue
        merged = commander_default(lid)
        merged.update(current)
        merged["shield_maiden"] = lid in SHIELD_MAIDEN_LIDS
        merged["epoch_available"] = clean_epoch(merged.get("epoch_available"))
        state[key] = merged
    return state


def load_rbc_state() -> dict[str, Any]:
    state = load_json(RBC_STATE_PATH, {})
    for key, current in list(state.items()):
        if not isinstance(current, dict) or key.startswith("_"):
            continue
        x, y = parse_xy_key(key)
        merged = {
            "x": x,
            "y": y,
            "kid": KINGDOM_ID,
            "level": current.get("level"),
            "estimated_level": current.get("estimated_level"),
            "gaa_value": current.get("gaa_value"),
            "distance": distance_from_source(x, y),
            "epoch_available": clean_epoch(current.get("epoch_available")),
            "under_attack": bool(current.get("under_attack", False)),
            "LID": current.get("LID"),
            "MID": current.get("MID"),
            "pending_adi": False,
            "pending_cra_after": 0.0,
            "last_adi_epoch": 0.0,
            "last_attack_epoch": 0.0,
            "last_error": None,
            "last_status": None,
            "consecutive_fail_count": 0,
            "last_fail_epoch": 0.0,
        }
        merged.update(current)
        merged["epoch_available"] = clean_epoch(merged.get("epoch_available"))
        merged["distance"] = distance_from_source(merged.get("x"), merged.get("y"))
        computed_level = sands_level_from_gaa_value(merged.get("gaa_value"))
        if computed_level is not None:
            merged["estimated_level"] = computed_level
            if float(merged.get("last_adi_epoch", 0.0) or 0.0) <= 0.0:
                merged["level"] = None
                merged.pop("level_source", None)
            elif merged.get("level") is not None and merged.get("level_source") is None:
                merged["level_source"] = "adi_gaa_value"
        state[key] = merged
    return state


def parse_xy_key(key: str) -> tuple[int | None, int | None]:
    try:
        x_text, y_text = key.split(":", 1)
        return int(x_text), int(y_text)
    except (TypeError, ValueError):
        return None, None


def xy_key(x: int, y: int) -> str:
    return f"{int(x)}:{int(y)}"


def distance_from_source(x: int | None, y: int | None) -> float | None:
    if x is None or y is None:
        return None
    return round(math.hypot(int(x) - SOURCE_X, int(y) - SOURCE_Y), 2)


def append_to_proxy(packet: str) -> None:
    PROXY_SEND_FILE.parent.mkdir(parents=True, exist_ok=True)
    with PROXY_SEND_FILE.open("a", encoding="utf-8") as handle:
        handle.write(packet.rstrip() + "\n")


def request_cooldown_remaining(commander_state: dict[str, Any], *, now: float | None = None) -> float:
    meta = commander_state.setdefault("_meta", {})
    last_request_epoch = float(meta.get("last_request_epoch", 0.0) or 0.0)
    if now is None:
        now = now_epoch()
    return max(0.0, REQUEST_COOLDOWN_SECONDS - (now - last_request_epoch))


def mark_request_sent(commander_state: dict[str, Any], *, now: float | None = None) -> None:
    commander_state.setdefault("_meta", {})["last_request_epoch"] = now_epoch() if now is None else now


def create_adi_packet(tx: int, ty: int, *, request_id: int = 1) -> str:
    payload = {"SX": SOURCE_X, "SY": SOURCE_Y, "TX": tx, "TY": ty, "KID": KINGDOM_ID}
    return "%xt%{}%adi%{}%{}%".format(
        SAND_SERVER_HEADER,
        request_id,
        json.dumps(payload, separators=(",", ":")),
    )


def create_gaa_packet(ax1: int, ay1: int, *, request_id: int = 1) -> str:
    payload = {
        "KID": KINGDOM_ID,
        "AX1": int(ax1),
        "AY1": int(ay1),
        "AX2": int(ax1) + MAP_CHUNK_SIZE - 1,
        "AY2": int(ay1) + MAP_CHUNK_SIZE - 1,
    }
    return "%xt%{}%gaa%{}%{}%".format(
        SAND_SERVER_HEADER,
        request_id,
        json.dumps(payload, separators=(",", ":")),
    )


def create_attack_packet(tx: int, ty: int, lid: int, attack: Attack, *, request_id: int = 1) -> str:
    payload = {
        "SX": SOURCE_X,
        "SY": SOURCE_Y,
        "TX": int(tx),
        "TY": int(ty),
        "KID": KINGDOM_ID,
        "LID": int(lid),
        "WT": 0,
        "HBW": HBW_VALUE,
        "BPC": 0,
        "ATT": 0,
        "AV": 0,
        "LP": 0,
        "FC": 0,
        "PTT": 0,
        "SD": 0,
        "ICA": 0,
        "CD": 99,
        "A": attack.to_payload(),
        "BKS": [],
        "AST": [-1, -1, -1],
        "RW": [[-1, 0] for _ in range(8)],
        "ASCT": 0,
    }
    return "%xt%{}%cra%{}%{}%".format(
        SAND_SERVER_HEADER,
        request_id,
        json.dumps(payload, separators=(",", ":")),
    )


def attack_for_level(level: int | None) -> Attack | None:
    if level is None:
        return None
    level = int(level)
    if level == LV61:
        return LEVEL_61
    if level <= 50:
        return NOT_LV_50_AND_BELOW
    return NOT_LV_61


def choose_lid(commander_state: dict[str, Any], level: int) -> int | None:
    lids = LV61_LIDS if level == LV61 else NON_LV61_LIDS
    now = now_epoch()
    for lid in lids:
        state = commander_state[str(lid)]
        if state.get("in_use"):
            continue
        if float(state.get("epoch_available", 0.0) or 0.0) <= now:
            return lid
    return None


def fallback_return_seconds_from_travel(travel_seconds: float | int | None) -> float:
    try:
        travel = float(travel_seconds or 0.0)
    except (TypeError, ValueError):
        travel = 0.0
    if travel <= 0.0:
        return BACKUP_LID_RETURN_SECONDS + random_between(RETURN_RANDOM_HOLD_RANGE)
    return travel * FALLBACK_RETURN_TRAVEL_MULTIPLIER + random_between(RETURN_RANDOM_HOLD_RANGE)


def mark_target_failure(rbc: dict[str, Any], *, now: float, status: str) -> tuple[int, float]:
    fail_count = int(rbc.get("consecutive_fail_count", 0) or 0) + 1
    base_timeout = min(
        TARGET_FAIL_BACKOFF_MAX_SECONDS,
        TARGET_FAIL_BACKOFF_BASE_SECONDS * (2 ** min(fail_count - 1, 10)),
    )
    timeout_seconds = base_timeout * random_between(TARGET_FAIL_BACKOFF_JITTER_RANGE)
    rbc.update(
        {
            "under_attack": False,
            "LID": None,
            "MID": None,
            "epoch_available": now + timeout_seconds,
            "pending_cra_after": 0.0,
            "last_error": status,
            "last_status": status,
            "consecutive_fail_count": fail_count,
            "last_fail_epoch": now,
        }
    )
    return fail_count, timeout_seconds


def clear_stale_lid_targets(
    rbc_state: dict[str, Any],
    *,
    lid: int,
    current_target: str | None,
    now: float,
    status: str,
) -> bool:
    changed = False
    for key, rbc in rbc_state.items():
        if key.startswith("_") or not isinstance(rbc, dict) or key == current_target:
            continue
        if int(rbc.get("LID") or -1) != int(lid) or not rbc.get("under_attack"):
            continue
        rbc.update(
            {
                "under_attack": False,
                "pending_cra_after": 0.0,
                "last_status": status,
            }
        )
        if float(rbc.get("epoch_available", 0.0) or 0.0) <= now:
            rbc["epoch_available"] = now + rbc_cooldown_addition()
        log(f"clear stale target={key} lid={lid} status={status}")
        changed = True
    return changed


def reconcile_active_state(commander_state: dict[str, Any], rbc_state: dict[str, Any]) -> bool:
    changed = False
    now = now_epoch()
    for key, rbc in rbc_state.items():
        if key.startswith("_") or not isinstance(rbc, dict) or not rbc.get("under_attack"):
            continue
        lid = rbc.get("LID")
        if lid is None:
            continue
        lid_state = commander_state.get(str(int(lid)))
        if not isinstance(lid_state, dict):
            continue
        current_target = lid_state.get("target") if lid_state.get("in_use") else None
        current_mid = lid_state.get("MID")
        rbc_mid = rbc.get("MID")
        stale_target = current_target != key
        stale_mid = rbc_mid is not None and current_mid is not None and int(rbc_mid) != int(current_mid)
        lid_not_attacking = lid_state.get("status") not in {"pending_cra", "outbound", "returning"}
        if not (stale_target or stale_mid or lid_not_attacking):
            continue
        rbc.update(
            {
                "under_attack": False,
                "pending_cra_after": 0.0,
                "last_status": "cleared_by_reconcile_active_state",
            }
        )
        if float(rbc.get("epoch_available", 0.0) or 0.0) <= now:
            rbc["epoch_available"] = now + rbc_cooldown_addition()
        log(
            "reconcile stale active "
            f"target={key} lid={lid} mid={rbc_mid} "
            f"current_target={current_target} current_mid={current_mid}"
        )
        changed = True
    return changed


XT_PACKET_RE = re.compile(r"(%xt%[^\n\r]+%)")


def extract_raw_packet(text: str) -> str | None:
    match = re.search(r"(?ms)^\s*raw:\s*\n(?P<packet>%xt%[^\n]+%)\s*\n---", text)
    if match:
        return match.group("packet").strip()
    match = XT_PACKET_RE.search(text)
    if match:
        return match.group(1).strip()
    return None


def parse_xt_packet(packet: str) -> dict[str, Any] | None:
    fields = packet.strip().strip("%").split("%")
    if len(fields) < 4 or fields[0] != "xt":
        return None
    if fields[1].startswith("EmpireEx_"):
        if len(fields) < 5:
            return None
        server_header = fields[1]
        command = fields[2]
        request_id = fields[3]
        status = None
        payload_text = fields[4]
    else:
        server_header = None
        command = fields[1]
        request_id = fields[2]
        status = fields[3]
        payload_text = fields[4] if len(fields) >= 5 else ""
    if payload_text:
        try:
            payload = json.loads(payload_text)
        except json.JSONDecodeError:
            payload = payload_text
    else:
        payload = None
    return {
        "server_header": server_header,
        "command": command,
        "request_id": request_id,
        "status": status,
        "payload": payload,
        "raw": packet,
    }


def file_epoch(path: Path) -> float:
    try:
        return datetime.strptime(path.stem, "%Y%m%d_%H%M%S_%f").timestamp()
    except ValueError:
        return path.stat().st_mtime


def parse_message_file(path: Path) -> dict[str, Any] | None:
    text = path.read_text(encoding="utf-8", errors="replace")
    packet = extract_raw_packet(text)
    if packet is None:
        return None
    parsed = parse_xt_packet(packet)
    if parsed is None:
        return None
    parsed["path"] = str(path)
    parsed["epoch"] = file_epoch(path)
    return parsed


def adi_level(payload: dict[str, Any]) -> int | None:
    for row in payload.get("AE") or []:
        if isinstance(row, list) and len(row) >= 3 and row[2] == "TL":
            try:
                return int(row[0])
            except (TypeError, ValueError):
                return None
    return None


def adi_gaa(payload: dict[str, Any]) -> tuple[int | None, int | None, int | None]:
    ai = (payload.get("gaa") or {}).get("AI")
    if not isinstance(ai, list) or len(ai) < 5:
        return None, None, None
    try:
        return int(ai[1]), int(ai[2]), int(ai[4])
    except (TypeError, ValueError):
        return None, None, None


def process_gaa(parsed: dict[str, Any], rbc_state: dict[str, Any]) -> bool:
    payload = parsed.get("payload")
    if not isinstance(payload, dict):
        return False
    changed = False
    for row in payload.get("AI") or []:
        if not isinstance(row, list) or len(row) < 5:
            continue
        try:
            area_type = int(row[0])
            x = int(row[1])
            y = int(row[2])
            gaa_value = int(row[4])
        except (TypeError, ValueError):
            continue
        if area_type != AREA_BARRON:
            continue
        key = xy_key(x, y)
        distance = distance_from_source(x, y)
        if distance is None or distance > MAX_TARGET_DISTANCE:
            continue
        current = rbc_state.get(key)
        if not isinstance(current, dict):
            current = {
                "x": x,
                "y": y,
                "kid": KINGDOM_ID,
                "level": None,
                "estimated_level": sands_level_from_gaa_value(gaa_value),
                "distance": distance,
                "epoch_available": 0.0,
                "under_attack": False,
                "LID": None,
                "MID": None,
                "pending_adi": False,
                "pending_cra_after": 0.0,
                "last_adi_epoch": 0.0,
                "last_attack_epoch": 0.0,
                "last_error": None,
                "last_status": None,
            }
        before = current.copy()
        current.update(
            {
                "x": x,
                "y": y,
                "kid": KINGDOM_ID,
                "gaa_value": gaa_value,
                "estimated_level": sands_level_from_gaa_value(gaa_value),
                "distance": distance,
                "last_gaa_epoch": float(parsed.get("epoch", now_epoch())),
            }
        )
        rbc_state[key] = current
        changed = changed or current != before
        if current != before:
            log(
                "gaa target "
                f"{key} est_lv={current.get('estimated_level')} "
                f"dist={current.get('distance')}"
            )
    return changed


def process_adi(parsed: dict[str, Any], rbc_state: dict[str, Any]) -> bool:
    payload = parsed.get("payload")
    if not isinstance(payload, dict):
        return False
    x, y, gaa_value = adi_gaa(payload)
    if x is None or y is None:
        return False
    key = xy_key(x, y)
    distance = distance_from_source(x, y)
    if distance is None or distance > MAX_TARGET_DISTANCE:
        return False
    current = rbc_state.get(key, {})
    if not isinstance(current, dict):
        current = {}
    was_waiting_for_adi = (
        bool(current.get("pending_adi"))
        or current.get("last_status") == "waiting_for_adi_level"
        or current.get("last_error") == "waiting_for_adi_level"
    )
    seen_at = float(parsed.get("epoch", now_epoch()))
    adi_tl = adi_level(payload)
    level = sands_level_from_gaa_value(gaa_value)
    if level is None:
        level = adi_tl
    current.update(
        {
            "x": x,
            "y": y,
            "kid": KINGDOM_ID,
            "level": level,
            "level_source": "adi_gaa_value" if gaa_value is not None else "adi_tl",
            "estimated_level": sands_level_from_gaa_value(gaa_value),
            "adi_tl": adi_tl,
            "gaa_value": gaa_value,
            "distance": distance,
            "last_adi_epoch": seen_at,
            "last_error": None,
            "last_status": "adi_level_known" if level is not None else "adi_no_level",
            "pending_adi": False,
        }
    )
    if was_waiting_for_adi:
        current["epoch_available"] = min(float(current.get("epoch_available", 0.0) or 0.0), now_epoch())
    if level is not None and current.get("pending_cra_after", 0.0) == 0.0:
        current["pending_cra_after"] = now_epoch() + random_between(ADI_TO_ATTACK_DELAY_RANGE)
    rbc_state[key] = current
    log(
        "adi target "
        f"{key} level={level} est={current.get('estimated_level')} "
        f"cra_after={int(current.get('pending_cra_after', 0) or 0)}"
    )
    return True


def movement_area_key(movement: dict[str, Any], field: str) -> str | None:
    area = movement.get(field)
    if isinstance(area, list) and len(area) >= 3:
        try:
            return xy_key(int(area[1]), int(area[2]))
        except (TypeError, ValueError):
            return None
    return None


def movement_target_key(movement: dict[str, Any], *, fields: tuple[str, ...]) -> str | None:
    for field in fields:
        key = movement_area_key(movement, field)
        if key is not None:
            return key
    return None


def process_cra_ack(
    parsed: dict[str, Any],
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any],
) -> bool:
    status = parsed.get("status")
    if status not in (None, "0", 0):
        return process_cra_error(parsed, commander_state, rbc_state)
    payload = parsed.get("payload")
    if not isinstance(payload, dict):
        return False
    attack = payload.get("AAM")
    if not isinstance(attack, dict):
        return False
    movement = attack.get("M")
    if not isinstance(movement, dict):
        return False
    target_key = movement_target_key(movement, fields=("TA",))
    if target_key not in rbc_state:
        return False
    march_id = movement.get("MID")
    lord_id = ((attack.get("UM") or {}).get("L") or {}).get("ID")
    if lord_id is None:
        lord_id = rbc_state[target_key].get("LID")
    if lord_id is None or march_id is None:
        return False
    lid_key = str(int(lord_id))
    seen_at = float(parsed.get("epoch", now_epoch()))
    travel_seconds = float(movement.get("TT", 0.0) or 0.0)
    fallback_return_epoch = seen_at + fallback_return_seconds_from_travel(travel_seconds)
    clear_stale_lid_targets(
        rbc_state,
        lid=int(lord_id),
        current_target=target_key,
        now=seen_at,
        status="cleared_by_new_cra_ack_same_lid",
    )
    commander_state[lid_key].update(
        {
            "epoch_available": fallback_return_epoch,
            "in_use": True,
            "status": "outbound",
            "MID": int(march_id),
            "target": target_key,
            "backup_return": True,
            "last_travel_seconds": travel_seconds,
            "last_update": seen_at,
        }
    )
    rbc_state[target_key].update(
        {
            "MID": int(march_id),
            "LID": int(lord_id),
            "under_attack": True,
            "pending_cra_after": 0.0,
            "consecutive_fail_count": 0,
            "last_error": None,
            "last_status": "cra_ack",
        }
    )
    log(
        "cra ack "
        f"target={target_key} lid={lord_id} mid={march_id} "
        f"travel={int(travel_seconds)} fallback_return={int(fallback_return_epoch)}"
    )
    return True


def newest_pending_cra(
    commander_state: dict[str, Any],
    *,
    seen_at: float | None = None,
    max_age: float = 240.0,
) -> tuple[str, dict[str, Any]] | tuple[None, None]:
    pending = [
        (lid, state)
        for lid, state in commander_state.items()
        if not lid.startswith("_")
        and isinstance(state, dict)
        and state.get("status") == "pending_cra"
        and state.get("MID") is None
    ]
    if seen_at is not None:
        pending = [
            (lid, state)
            for lid, state in pending
            if 0 <= seen_at - float(state.get("last_update", 0.0) or 0.0) <= max_age
        ]
    if not pending:
        return None, None
    return max(pending, key=lambda item: float(item[1].get("last_update", 0.0) or 0.0))


def has_pending_cra(commander_state: dict[str, Any]) -> bool:
    lid, _ = newest_pending_cra(commander_state)
    return lid is not None


def process_cra_error(
    parsed: dict[str, Any],
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any],
) -> bool:
    now = float(parsed.get("epoch", now_epoch()))
    lid, state = newest_pending_cra(commander_state, seen_at=now)
    if lid is None or state is None:
        return False
    target = state.get("target")
    if target in rbc_state and isinstance(rbc_state[target], dict):
        fail_count, timeout_seconds = mark_target_failure(
            rbc_state[target],
            now=now,
            status=f"cra_status_{parsed.get('status')}",
        )
    else:
        fail_count = 0
        timeout_seconds = 0.0
    state.update(
        {
            "epoch_available": now + BACKUP_LID_RETURN_SECONDS + random_between(RETURN_RANDOM_HOLD_RANGE),
            "in_use": True,
            "status": "busy_after_cra_error",
            "MID": None,
            "target": None,
            "backup_return": True,
            "last_update": now,
            "last_error": f"cra_status_{parsed.get('status')}",
        }
    )
    log(
        "cra error "
        f"status={parsed.get('status')} lid={lid} target={target} "
        f"fail_count={fail_count} target_backoff={int(timeout_seconds)}s "
        f"lid_retry_after={int(state.get('epoch_available', 0))}"
    )
    return True


def process_cat(
    parsed: dict[str, Any],
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any],
) -> bool:
    payload = parsed.get("payload")
    if not isinstance(payload, dict):
        return False
    attack = payload.get("A")
    if not isinstance(attack, dict):
        return False
    movement = attack.get("M")
    if not isinstance(movement, dict):
        return False
    target_key = movement_target_key(movement, fields=("SA", "TA"))
    if target_key not in rbc_state:
        return False
    march_id = movement.get("MID")
    lord_id = ((attack.get("UM") or {}).get("L") or {}).get("ID")
    if lord_id is None:
        lord_id = rbc_state[target_key].get("LID")
    seen_at = float(parsed.get("epoch", now_epoch()))
    travel_seconds = float(movement.get("TT", BACKUP_LID_RETURN_SECONDS) or BACKUP_LID_RETURN_SECONDS)
    if lord_id is None:
        return False
    lid_key = str(int(lord_id))
    return_epoch = seen_at + travel_seconds + random_between(RETURN_RANDOM_HOLD_RANGE)
    lid_state = commander_state[lid_key]
    march_id_int = int(march_id) if march_id is not None else None
    should_update_lid = (
        lid_state.get("target") == target_key
        or (march_id_int is not None and lid_state.get("MID") == march_id_int)
    )
    if should_update_lid:
        lid_state.update(
            {
                "epoch_available": return_epoch,
                "in_use": True,
                "status": "returning",
                "MID": march_id_int if march_id_int is not None else lid_state.get("MID"),
                "target": target_key,
                "backup_return": False,
                "last_travel_seconds": travel_seconds,
                "last_update": seen_at,
            }
        )
    else:
        log(
            "cat old movement "
            f"target={target_key} lid={lord_id} mid={march_id} "
            f"current_target={lid_state.get('target')} current_mid={lid_state.get('MID')}"
        )
    rbc_state[target_key].update(
        {
            "epoch_available": seen_at + rbc_cooldown_addition(),
            "under_attack": False,
            "MID": march_id,
            "LID": int(lord_id),
            "pending_cra_after": 0.0,
            "consecutive_fail_count": 0,
            "last_error": None,
            "last_status": "cat_returning",
        }
    )
    log(
        "cat return "
        f"target={target_key} lid={lord_id} mid={march_id} "
        f"return_at={int(return_epoch)} rbc_ready={int(rbc_state[target_key].get('epoch_available', 0))}"
    )
    return True


def process_dms(parsed: dict[str, Any], commander_state: dict[str, Any]) -> bool:
    payload = parsed.get("payload")
    if not isinstance(payload, dict):
        return False
    mids = payload.get("MID") or []
    changed = False
    seen_at = float(parsed.get("epoch", now_epoch()))
    for mid in mids:
        for state in commander_state.values():
            if not isinstance(state, dict) or state.get("MID") != int(mid):
                continue
            state.update(
                {
                    "epoch_available": seen_at + random_between(RETURN_RANDOM_HOLD_RANGE),
                    "in_use": True,
                    "status": "returning",
                    "backup_return": False,
                    "last_update": seen_at,
                }
            )
            log(f"dms returning mid={mid} release_at={int(state.get('epoch_available', 0))}")
            changed = True
    return changed


def release_returned_commanders(
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any] | None = None,
) -> bool:
    changed = False
    now = now_epoch()
    for lid, state in commander_state.items():
        if lid.startswith("_") or not isinstance(state, dict):
            continue
        if state.get("in_use") and float(state.get("epoch_available", 0.0) or 0.0) <= now:
            target = state.get("target")
            if (
                rbc_state is not None
                and target in rbc_state
                and isinstance(rbc_state[target], dict)
                and rbc_state[target].get("under_attack")
            ):
                rbc_state[target].update(
                    {
                        "under_attack": False,
                        "pending_cra_after": 0.0,
                        "last_status": "cleared_by_fallback_return",
                    }
                )
                if float(rbc_state[target].get("epoch_available", 0.0) or 0.0) <= now:
                    rbc_state[target]["epoch_available"] = now + rbc_cooldown_addition()
            state.update(
                {
                    "in_use": False,
                    "status": "available",
                    "MID": None,
                    "target": None,
                    "backup_return": False,
                    "last_update": now,
                }
            )
            log(f"lid available lid={lid}")
            changed = True
    return changed


def release_stale_pending_cra(
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any],
) -> bool:
    changed = False
    now = now_epoch()
    for lid, state in commander_state.items():
        if lid.startswith("_") or not isinstance(state, dict):
            continue
        if state.get("status") != "pending_cra" or state.get("MID") is not None:
            continue
        last_update = float(state.get("last_update", 0.0) or 0.0)
        if now - last_update < random_between(PENDING_CRA_TIMEOUT_RANGE):
            continue
        target = state.get("target")
        if target in rbc_state and isinstance(rbc_state[target], dict):
            fail_count, timeout_seconds = mark_target_failure(
                rbc_state[target],
                now=now,
                status="pending_cra_timeout_no_ack",
            )
            state.update(
                {
                    "epoch_available": now + random_between(RETURN_RANDOM_HOLD_RANGE),
                    "in_use": False,
                    "status": "available",
                    "MID": None,
                    "target": None,
                    "backup_return": False,
                    "last_update": now,
                }
            )
            log(
                "pending cra timeout "
                f"lid={lid} target={target} fail_count={fail_count} "
                f"target_backoff={int(timeout_seconds)}s"
            )
        changed = True
    return changed


def process_server_packet(
    parsed: dict[str, Any],
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any],
) -> bool:
    command = parsed.get("command")
    if command == "gaa":
        return process_gaa(parsed, rbc_state)
    if command == "adi":
        return process_adi(parsed, rbc_state)
    if command == "cra":
        return process_cra_ack(parsed, commander_state, rbc_state)
    if command == "cat":
        return process_cat(parsed, commander_state, rbc_state)
    if command == "dms":
        return process_dms(parsed, commander_state)
    return False


def process_latest_server_messages(
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any],
) -> bool:
    if not LATEST_SERVER_MESSAGES.exists():
        return False
    meta = commander_state.setdefault("_meta", {})
    seen = set(meta.get("seen_message_files", []))
    changed = False
    for path in sorted(LATEST_SERVER_MESSAGES.glob("*")):
        if not path.is_file() or str(path) in seen:
            continue
        parsed = parse_message_file(path)
        seen.add(str(path))
        if parsed is None:
            continue
        changed = process_server_packet(parsed, commander_state, rbc_state) or changed
    meta["seen_message_files"] = sorted(seen)[-200:]
    return changed


def newest_capture_logs(limit: int = 20) -> list[Path]:
    if not CAPTURE_LOGS_DIR.exists():
        return []
    paths = [path for path in CAPTURE_LOGS_DIR.glob("gge_*.log") if path.is_file()]
    return sorted(paths, key=lambda path: path.stat().st_mtime)[-limit:]


def process_capture_logs(
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any],
) -> bool:
    meta = commander_state.setdefault("_meta", {})
    offsets = meta.setdefault("capture_offsets", {})
    changed = False
    for path in newest_capture_logs():
        path_key = str(path)
        size = path.stat().st_size
        previous_offset = offsets.get(path_key)
        if previous_offset is None:
            previous_offset = 0
        offset = min(int(previous_offset), size)
        if offset == size:
            continue
        with path.open("r", encoding="utf-8", errors="replace") as handle:
            handle.seek(offset)
            text = handle.read()
            offsets[path_key] = handle.tell()
        for packet in XT_PACKET_RE.findall(text):
            parsed = parse_xt_packet(packet)
            if parsed is None:
                continue
            parsed["path"] = path_key
            parsed["epoch"] = path.stat().st_mtime
            changed = process_server_packet(parsed, commander_state, rbc_state) or changed
    meta["capture_offsets"] = {key: offsets[key] for key in sorted(offsets)[-60:]}
    return changed


def replay_recent_gaa_from_captures(
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any],
    *,
    max_logs: int = 8,
) -> bool:
    changed = False
    for path in newest_capture_logs(limit=max_logs):
        text = path.read_text(encoding="utf-8", errors="replace")
        for packet in XT_PACKET_RE.findall(text):
            parsed = parse_xt_packet(packet)
            if parsed is None or parsed.get("command") != "gaa":
                continue
            parsed["path"] = str(path)
            parsed["epoch"] = path.stat().st_mtime
            changed = process_gaa(parsed, rbc_state) or changed
    offsets = commander_state.setdefault("_meta", {}).setdefault("capture_offsets", {})
    for path in newest_capture_logs(limit=max_logs):
        offsets[str(path)] = path.stat().st_size
    return changed


def scan_chunk_plan(radius: int = MAP_SCAN_RADIUS) -> list[tuple[int, int]]:
    start_x = SOURCE_X - radius
    start_y = SOURCE_Y - radius
    width = radius * 2 + 1
    chunks = [
        (start_x + ox, start_y + oy)
        for ox in range(0, width, MAP_CHUNK_SIZE)
        for oy in range(0, width, MAP_CHUNK_SIZE)
    ]
    chunks.sort(key=lambda item: math.hypot(item[0] + 6 - SOURCE_X, item[1] + 6 - SOURCE_Y))
    jittered = []
    band_size = random.randint(3, 6)
    for index in range(0, len(chunks), band_size):
        band = chunks[index : index + band_size]
        random.shuffle(band)
        jittered.extend(band)
    if random.random() < 0.35:
        jittered.reverse()
    return jittered


def next_scan_chunk(commander_state: dict[str, Any], *, radius: int = MAP_SCAN_RADIUS) -> tuple[int, int]:
    meta = commander_state.setdefault("_meta", {})
    plan = scan_chunk_plan(radius)
    cursor = int(meta.get("scan_cursor", random.randrange(len(plan))) or 0) % len(plan)
    step = random.choice((1, 1, 1, 2, 3))
    chunk = plan[cursor]
    meta["scan_cursor"] = (cursor + step) % len(plan)
    meta["next_refresh_scan_epoch"] = now_epoch() + random_between(REFRESH_SCAN_DELAY_RANGE)
    return chunk


def queue_scan_chunk(
    commander_state: dict[str, Any],
    *,
    radius: int = MAP_SCAN_RADIUS,
    dry_run: bool = False,
) -> tuple[int, int]:
    cooldown_remaining = request_cooldown_remaining(commander_state)
    if cooldown_remaining > 0 and not dry_run:
        wait_for = cooldown_remaining + random.uniform(0.2, 1.8)
        log(f"request cooldown before gaa wait={wait_for:.1f}s")
        time.sleep(wait_for)
    ax1, ay1 = next_scan_chunk(commander_state, radius=radius)
    packet = create_gaa_packet(ax1, ay1)
    if not dry_run:
        append_to_proxy(packet)
        mark_request_sent(commander_state)
    log(f"send gaa chunk={ax1}:{ay1}")
    return ax1, ay1


def maybe_refresh_scan(commander_state: dict[str, Any], *, dry_run: bool = False) -> bool:
    meta = commander_state.setdefault("_meta", {})
    if float(meta.get("next_refresh_scan_epoch", 0.0) or 0.0) > now_epoch():
        return False
    queue_scan_chunk(commander_state, dry_run=dry_run)
    return True


def target_sort_key(item: tuple[str, dict[str, Any]]) -> tuple[float, int, float, float]:
    _, rbc = item
    now = now_epoch()
    epoch_available = clean_epoch(rbc.get("epoch_available"))
    pending_cra_after = clean_epoch(rbc.get("pending_cra_after"))
    action_epoch = max(epoch_available, pending_cra_after)
    wait_remaining = max(0.0, action_epoch - now)
    level = rbc.get("level")
    needs_adi = 1 if level is None else 0
    distance = float(rbc.get("distance") or 9999.0)
    distance_jitter = random.uniform(-2.75, 2.75)
    return wait_remaining, needs_adi, distance + distance_jitter, random.random()


def eligible_targets(rbc_state: dict[str, Any]) -> list[tuple[str, dict[str, Any]]]:
    targets = [
        (key, rbc)
        for key, rbc in rbc_state.items()
        if not key.startswith("_") and isinstance(rbc, dict)
        and float(rbc.get("distance") or MAX_TARGET_DISTANCE + 1.0) <= MAX_TARGET_DISTANCE
    ]
    return sorted(targets, key=target_sort_key)


def setup_scan(count: int, *, radius: int = MAP_SCAN_RADIUS, dry_run: bool = False) -> None:
    commander_state = load_commander_state()
    rbc_state = load_rbc_state()
    process_latest_server_messages(commander_state, rbc_state)
    process_capture_logs(commander_state, rbc_state)
    if not dry_run:
        save_json(COMMANDER_STATE_PATH, commander_state)
        save_json(RBC_STATE_PATH, rbc_state)
    for index in range(max(0, count)):
        ax1, ay1 = queue_scan_chunk(commander_state, radius=radius, dry_run=dry_run)
        if not dry_run:
            save_json(COMMANDER_STATE_PATH, commander_state)
        print(f"queued gaa scan {index + 1}/{count}: {ax1}:{ay1}")
        time.sleep(random_between(SETUP_SCAN_DELAY_RANGE))
        changed = process_latest_server_messages(commander_state, rbc_state)
        changed = process_capture_logs(commander_state, rbc_state) or changed
        if changed and not dry_run:
            save_json(COMMANDER_STATE_PATH, commander_state)
            save_json(RBC_STATE_PATH, rbc_state)
    changed = process_latest_server_messages(commander_state, rbc_state)
    changed = process_capture_logs(commander_state, rbc_state) or changed
    if changed and not dry_run:
        save_json(COMMANDER_STATE_PATH, commander_state)
        save_json(RBC_STATE_PATH, rbc_state)


def replay_recent_gaa(*, dry_run: bool = False) -> None:
    commander_state = load_commander_state()
    rbc_state = load_rbc_state()
    changed = replay_recent_gaa_from_captures(commander_state, rbc_state)
    rbc_count = len([key for key in rbc_state if not key.startswith("_")])
    if changed and not dry_run:
        save_json(COMMANDER_STATE_PATH, commander_state)
        save_json(RBC_STATE_PATH, rbc_state)
    print(f"replayed recent gaa packets; changed={changed}; rbc_count={rbc_count}")


def schedule_adi_or_attack(
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any],
    *,
    dry_run: bool = False,
) -> bool:
    now = now_epoch()
    pending_lid, pending_state = newest_pending_cra(commander_state)
    pending_cra_logged = False
    cooldown_remaining = request_cooldown_remaining(commander_state, now=now)
    if cooldown_remaining > 0:
        log(f"blocked request cooldown wait={cooldown_remaining:.1f}s")
        return False
    for key, rbc in eligible_targets(rbc_state):
        x = rbc.get("x")
        y = rbc.get("y")
        if x is None or y is None:
            x, y = parse_xy_key(key)
        if x is None or y is None:
            continue
        if rbc.get("under_attack"):
            continue
        if float(rbc.get("epoch_available", 0.0) or 0.0) > now:
            continue

        level = rbc.get("level")
        if rbc.get("pending_adi"):
            rbc["pending_adi"] = False
            rbc["last_status"] = "adi_retry_open"
            log(f"adi retry open target={key} level={level}")
        if level is None:
            if not rbc.get("pending_adi"):
                packet = create_adi_packet(int(x), int(y))
                if not dry_run:
                    append_to_proxy(packet)
                    mark_request_sent(commander_state, now=now)
                log(f"send adi target={key} dist={rbc.get('distance')}")
                rbc.update(
                    {
                        "pending_adi": True,
                        "pending_cra_after": 0.0,
                        "epoch_available": now + random_between(UNKNOWN_ADI_RETRY_RANGE),
                        "last_status": "waiting_for_adi_level",
                    }
                )
                return True
            continue

        level = int(level)
        attack = attack_for_level(level)
        if attack is None:
            continue
        if float(rbc.get("pending_cra_after", 0.0) or 0.0) > now:
            continue
        if pending_lid is not None:
            if not pending_cra_logged:
                log(
                    "blocked cra pending_ack "
                    f"lid={pending_lid} target={pending_state.get('target') if pending_state else None}"
                )
                pending_cra_logged = True
            continue
        last_adi_epoch = float(rbc.get("last_adi_epoch", 0.0) or 0.0)
        stale_adi = now - last_adi_epoch > ADI_REFRESH_AFTER_SECONDS
        random_refresh = (
            now - last_adi_epoch > ADI_REFRESH_AFTER_SECONDS / 3
            and random.random() < ADI_REFRESH_PROBABILITY
        )
        if not rbc.get("pending_adi") and (stale_adi or random_refresh):
            packet = create_adi_packet(int(x), int(y))
            if not dry_run:
                append_to_proxy(packet)
                mark_request_sent(commander_state, now=now)
            log(
                "send adi refresh "
                f"target={key} level={level} dist={rbc.get('distance')} stale={stale_adi}"
            )
            rbc.update(
                {
                    "pending_adi": True,
                    "pending_cra_after": 0.0,
                    "epoch_available": now + random_between(UNKNOWN_ADI_RETRY_RANGE),
                    "last_status": "refreshing_adi_level",
                }
            )
            return True
        lid = choose_lid(commander_state, level)
        if lid is None:
            if level == LV61:
                rbc.update(
                    {
                        "epoch_available": now + random_between(NO_LV61_LID_RETRY_RANGE),
                        "last_status": "no_valid_0_2_to_9_lid_for_lv61",
                    }
                )
                log(f"no valid 0/2-9 lv61 lid target={key} retry_at={int(rbc.get('epoch_available', 0))}")
                return True
            continue
        if level == LV61 and lid not in SHIELD_MAIDEN_LIDS:
            rbc.update(
                {
                    "epoch_available": now + random_between(NO_LV61_LID_RETRY_RANGE),
                    "last_status": f"blocked_bad_lv61_lid_{lid}",
                }
            )
            log(f"blocked bad lv61 lid target={key} lid={lid}")
            return True

        packet = create_attack_packet(int(x), int(y), lid, attack)
        if not dry_run:
            append_to_proxy(packet)
            mark_request_sent(commander_state, now=now)
        attack_name = "LEVEL_61" if attack is LEVEL_61 else "LOW_50" if attack is NOT_LV_50_AND_BELOW else "NOT_LV_61"
        log(f"send cra target={key} level={level} lid={lid} attack={attack_name}")
        commander_state[str(lid)].update(
            {
                "epoch_available": now + BACKUP_LID_RETURN_SECONDS + random_between(RETURN_RANDOM_HOLD_RANGE),
                "in_use": True,
                "status": "pending_cra",
                "MID": None,
                "target": key,
                "backup_return": True,
                "last_update": now,
            }
        )
        rbc.update(
            {
                "under_attack": True,
                "LID": lid,
                "pending_adi": False,
                "pending_cra_after": 0.0,
                "last_attack_epoch": now,
                "last_error": None,
                "last_status": None,
            }
        )
        return True
    log("idle no eligible target")
    return False


def run_once(*, dry_run: bool = False) -> None:
    log(f"run_once dry_run={dry_run}")
    commander_state = load_commander_state()
    rbc_state = load_rbc_state()
    changed = process_latest_server_messages(commander_state, rbc_state)
    changed = process_capture_logs(commander_state, rbc_state) or changed
    changed = release_stale_pending_cra(commander_state, rbc_state) or changed
    changed = reconcile_active_state(commander_state, rbc_state) or changed
    changed = release_returned_commanders(commander_state, rbc_state) or changed
    changed = reconcile_active_state(commander_state, rbc_state) or changed

    sent_or_updated = schedule_adi_or_attack(commander_state, rbc_state, dry_run=dry_run)
    if AUTO_REFRESH_SCAN and not sent_or_updated:
        sent_or_updated = maybe_refresh_scan(commander_state, dry_run=dry_run) or sent_or_updated
    changed = changed or sent_or_updated
    if changed and not dry_run:
        save_json(COMMANDER_STATE_PATH, commander_state)
        save_json(RBC_STATE_PATH, rbc_state)


def main() -> None:
    parser = argparse.ArgumentParser(description="Simple sand RBC farmer state loop.")
    parser.add_argument("--once", action="store_true", help="run one pass and exit")
    parser.add_argument("--dry-run", action="store_true", help="do not write packets or JSON")
    parser.add_argument("--sleep", type=float, default=STRICT_ATTACK_PACKET_COOLDOWN_MIN)
    parser.add_argument("--setup-scan", action="store_true", help="queue randomized GAA scan chunks and exit")
    parser.add_argument("--replay-recent-gaa", action="store_true", help="read recent capture logs and apply GAA responses")
    parser.add_argument("--scan-count", type=int, default=12, help="number of setup GAA chunks to queue")
    parser.add_argument("--scan-radius", type=int, default=MAP_SCAN_RADIUS, help="scan radius around source castle")
    parser.add_argument(
        "--timeout-minutes",
        type=float,
        default=REQUEST_SEQUENCE_TIMEOUT_MINUTES,
        help="stop queuing new request sequences after this many minutes; 0 disables",
    )
    args = parser.parse_args()

    if args.setup_scan:
        setup_scan(args.scan_count, radius=args.scan_radius, dry_run=args.dry_run)
        return

    if args.replay_recent_gaa:
        replay_recent_gaa(dry_run=args.dry_run)
        return

    if args.once:
        run_once(dry_run=args.dry_run)
        return

    started_at = now_epoch()
    timeout_seconds = max(0.0, args.timeout_minutes) * 60.0
    log(
        "loop start "
        f"sleep>={max(args.sleep, STRICT_ATTACK_PACKET_COOLDOWN_MIN):.1f}s "
        f"timeout_min={args.timeout_minutes}"
    )
    while True:
        if timeout_seconds and now_epoch() - started_at >= timeout_seconds:
            log(f"request sequence timeout reached after {args.timeout_minutes:.2f} minutes; stopping")
            return
        run_once(dry_run=args.dry_run)
        time.sleep(max(args.sleep, STRICT_ATTACK_PACKET_COOLDOWN_MIN) + random.uniform(0.0, 5.0))


if __name__ == "__main__":
    main()
