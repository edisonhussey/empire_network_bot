from __future__ import annotations

import argparse
import os
import random
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

SCAN_ROOT = Path(os.environ.get("GGE_SCAN_ROOT", "/Users/edisonhussey/Desktop/scan_coordinates"))
for path in (SCAN_ROOT, SCAN_ROOT / "pygge_repo"):
    if str(path) not in sys.path:
        sys.path.insert(0, str(path))

from event_worker.castles import find_castle_xy
from event_worker.gge_session import (
    LoginTemporarilyBlocked,
    connect_and_login,
    disconnect,
)
from event_worker.kingdoms import BURNING_SANDS

from empire.sand_rbc_farm import main as farm


HERE = Path(__file__).resolve().parent
DEFAULT_ACCOUNT = SCAN_ROOT / "ventrilo.ini"
DEFAULT_LOG_DIR = HERE / "logs"
LOG_FILE = None
ANSI_RED = "\033[31m"
ANSI_DIM = "\033[2m"
ANSI_RESET = "\033[0m"

LOGIN_COOLDOWN_PADDING_RANGE = (31.0, 91.0)
RECONNECT_BACKOFF_BASE_SECONDS = 300.0
RECONNECT_BACKOFF_MAX_SECONDS = 3600.0
SCAN_CHUNK_DELAY_RANGE = (10.4, 14.8)
IDLE_SLEEP_RANGE = (18.0, 35.0)
ERROR_BACKOFF_BASE_SECONDS = 120.0
ERROR_BACKOFF_MAX_SECONDS = 3600.0
MAX_RECENT_ERRORS = 20
ERROR_PAUSE_TRIGGER = 14
GLOBAL_ERROR_PAUSE_RANGE = (45 * 60, 90 * 60)
ATTACK_COOLDOWN_RANGE = (15.0, 35.0)
ATTACK_ERROR_COOLDOWN_RANGE = (8 * 60.0, 20 * 60.0)
MAX_ACTIVE_DIRECT = 35
MAX_DIRECT_CRA_PER_HOUR = 0
MAX_DIRECT_CRA_PER_SESSION = 0
CRA_RESULT_WINDOW_SIZE = 20
CRA_RESULT_WINDOW_STOP_ERRORS = 4
CONSECUTIVE_CRA_ERROR_STOP = 3
MOVEMENT_REFRESH_RANGE = (74.0, 143.0)
SERVER_ACTIVE_LID_HOLD_RANGE = (180.0, 360.0)
ACK_RETURN_MULTIPLIER_RANGE = (2.2, 2.8)
ACK_RETURN_HOLD_RANGE = (60.0, 180.0)
TARGET_LID_UNAVAILABLE_RETRY_RANGE = (12 * 60.0, 31 * 60.0)
TARGET_ADI_COOLDOWN_FALLBACK_RANGE = (2.7 * 3600.0, 3.4 * 3600.0)
TARGET_ADI_UNAVAILABLE_RETRY_RANGE = (18 * 60.0, 47 * 60.0)
LV61_QUEUE_PAUSE_AFTER_NO_LID_RANGE = (6 * 60.0, 14 * 60.0)
LV61_CROSSBOW_ATTACK_PROBABILITY = 0.5
NON61_EDGE_DISTANCE_TARGET = 36.0
LV61_COMMANDER_LIDS = farm.LV61_LIDS
NON_LV61_COMMANDER_LIDS = farm.NON_LV61_LIDS


class SessionClosed(RuntimeError):
    pass


class SafetyStop(RuntimeError):
    pass


def log(message: str, *, error: bool = False) -> None:
    timestamp = datetime.now().astimezone().isoformat(timespec="seconds")
    level = "ERROR" if error else "INFO"
    stream = sys.stderr if error else sys.stdout
    line = f"[sand-pygge] [{level}] {message} ts={timestamp}"
    if stream.isatty():
        if error:
            terminal_line = f"{ANSI_RED}{line}{ANSI_RESET}"
        else:
            terminal_line = f"{ANSI_DIM}[sand-pygge]{ANSI_RESET} [{level}] {message} ts={timestamp}"
    else:
        terminal_line = line
    print(terminal_line, file=stream, flush=True)
    if LOG_FILE is not None:
        print(line, file=LOG_FILE, flush=True)


def save_states(commander_state: dict[str, Any], rbc_state: dict[str, Any]) -> None:
    farm.save_json(farm.COMMANDER_STATE_PATH, commander_state)
    farm.save_json(farm.RBC_STATE_PATH, rbc_state)


def normalize_response(command: str, response: dict[str, Any], *, seen_at: float | None = None) -> dict[str, Any]:
    payload = response.get("payload", {}) if isinstance(response, dict) else {}
    status = payload.get("status")
    return {
        "server_header": None,
        "command": command,
        "request_id": None,
        "status": status,
        "payload": payload.get("data"),
        "raw": None,
        "epoch": farm.now_epoch() if seen_at is None else seen_at,
    }


def active_summary(commander_state: dict[str, Any]) -> str:
    active = [
        f"{lid}:{state.get('status')}->{state.get('target')}"
        for lid, state in commander_state.items()
        if not lid.startswith("_") and isinstance(state, dict) and state.get("in_use")
    ]
    max_active = len(farm.VALID_LIDS)
    return f"active={len(active)}/{max_active} outbound={','.join(active[:6]) if active else '-'}"


def active_count(commander_state: dict[str, Any]) -> int:
    return sum(
        1
        for lid, state in commander_state.items()
        if not lid.startswith("_") and isinstance(state, dict) and state.get("in_use")
    )


def known_food_from_response(response: dict[str, Any]) -> Any:
    data = response.get("payload", {}).get("data") if isinstance(response, dict) else {}
    if not isinstance(data, dict):
        return None
    for key in ("R", "res"):
        resources = data.get(key)
        if isinstance(resources, list) and len(resources) > 3:
            return resources[3]
        if isinstance(resources, dict):
            for food_key in ("food", "F", "3"):
                if food_key in resources:
                    return resources[food_key]
    return None


def connect_sources(account_config: Path):
    socket = connect_and_login(config_path=account_config)
    castles = socket.get_castles()
    sx, sy, cid = find_castle_xy(castles, kingdom=BURNING_SANDS)
    if (farm.SOURCE_X, farm.SOURCE_Y) != (int(sx), int(sy)):
        log(f"source_override configured={farm.SOURCE_X}:{farm.SOURCE_Y} login={sx}:{sy}")
        farm.SOURCE_X = int(sx)
        farm.SOURCE_Y = int(sy)
    nav = socket.go_to_castle(BURNING_SANDS, cid if cid is not None else -1)
    socket.open_map(BURNING_SANDS)
    food = known_food_from_response(nav)
    log(f"connected source={sx}:{sy} castle_id={cid} food={food}")
    return socket, sx, sy, cid


def connect_sources_with_cooldown(account_config: Path):
    while True:
        try:
            return connect_sources(account_config)
        except LoginTemporarilyBlocked as exc:
            server_wait = max(float(exc.retry_seconds or 0.0), 60.0)
            padding = random.uniform(*LOGIN_COOLDOWN_PADDING_RANGE)
            delay = server_wait + padding
            log(f"login_blocked wait={delay:.1f}s server_wait={server_wait:.0f}s", error=True)
            time.sleep(delay)


def reconnect_sources(old_socket, account_config: Path, failures: int):
    try:
        if old_socket is not None:
            disconnect(old_socket)
    except Exception:
        pass
    failures = min(8, failures + 1)
    raw_delay = min(RECONNECT_BACKOFF_MAX_SECONDS, RECONNECT_BACKOFF_BASE_SECONDS * (2 ** (failures - 1)))
    delay = raw_delay * random.uniform(0.85, 1.25)
    log(f"reconnect_wait={delay:.1f}s failures={failures}", error=True)
    time.sleep(delay)
    return (*connect_sources_with_cooldown(account_config), failures)


def socket_is_closed(socket) -> bool:
    return bool(getattr(socket, "closed", None) and socket.closed.is_set())


def is_closed_connection_error(exc: BaseException) -> bool:
    return "Connection is already closed" in str(exc) or "WebSocketConnectionClosedException" in type(exc).__name__


def request_wait(commander_state: dict[str, Any]) -> float:
    remaining = farm.request_cooldown_remaining(commander_state)
    if remaining > 0:
        wait = remaining + random.uniform(0.2, 1.8)
        log(f"cooldown wait={wait:.1f}s")
        time.sleep(wait)
    return remaining


def attack_wait(commander_state: dict[str, Any]) -> float:
    meta = commander_state.setdefault("_meta", {})
    next_attack_epoch = float(meta.get("next_pygge_attack_epoch", 0.0) or 0.0)
    remaining = max(0.0, next_attack_epoch - farm.now_epoch())
    if remaining > 0:
        wait = remaining + random.uniform(0.15, 1.35)
        log(f"attack_cooldown wait={wait:.4f}s")
        time.sleep(wait)
    return remaining


def mark_attack_sent(commander_state: dict[str, Any]) -> None:
    commander_state.setdefault("_meta", {})["next_pygge_attack_epoch"] = (
        farm.now_epoch() + farm.random_between(ATTACK_COOLDOWN_RANGE)
    )


def mark_attack_error_cooldown(commander_state: dict[str, Any]) -> float:
    meta = commander_state.setdefault("_meta", {})
    cooldown_until = farm.now_epoch() + farm.random_between(ATTACK_ERROR_COOLDOWN_RANGE)
    meta["next_pygge_attack_epoch"] = max(
        cooldown_until,
        float(meta.get("next_pygge_attack_epoch", 0.0) or 0.0),
    )
    return meta["next_pygge_attack_epoch"]


def choose_lid_from_pool(commander_state: dict[str, Any], lids: tuple[int, ...]) -> int | None:
    now = farm.now_epoch()
    for lid in lids:
        state = commander_state[str(lid)]
        if state.get("in_use"):
            continue
        if float(state.get("epoch_available", 0.0) or 0.0) <= now:
            return lid
    return None


def pool_has_ready_lid(commander_state: dict[str, Any], lids: tuple[int, ...]) -> bool:
    return choose_lid_from_pool(commander_state, lids) is not None


def queue_pause_key(queue_name: str) -> str:
    return f"next_pygge_{queue_name}_queue_epoch"


def queue_is_paused(commander_state: dict[str, Any], queue_name: str) -> bool:
    meta = commander_state.setdefault("_meta", {})
    return float(meta.get(queue_pause_key(queue_name), 0.0) or 0.0) > farm.now_epoch()


def pause_queue(commander_state: dict[str, Any], queue_name: str, seconds_range: tuple[float, float]) -> float:
    pause_until = farm.now_epoch() + farm.random_between(seconds_range)
    commander_state.setdefault("_meta", {})[queue_pause_key(queue_name)] = pause_until
    return pause_until


def row_lord_id(row: Any) -> int | None:
    try:
        if isinstance(row, dict):
            value = row.get("ID")
        elif isinstance(row, list) and row:
            value = row[0]
        else:
            return None
        return int(value)
    except (TypeError, ValueError):
        return None


def target_available_lids(target_info: dict[str, Any]) -> set[int]:
    data = target_info.get("payload", {}).get("data") if isinstance(target_info, dict) else {}
    if not isinstance(data, dict):
        return set()
    commanders = (data.get("gli") or {}).get("C") or []
    return {lid for lid in (row_lord_id(row) for row in commanders) if lid is not None}


def global_lids(lords_response: dict[str, Any]) -> set[int]:
    data = lords_response.get("payload", {}).get("data") if isinstance(lords_response, dict) else {}
    if not isinstance(data, dict):
        return set()
    commanders = data.get("C") or []
    return {lid for lid in (row_lord_id(row) for row in commanders) if lid is not None}


def active_lids(commander_state: dict[str, Any]) -> set[int]:
    found: set[int] = set()
    for lid, state in commander_state.items():
        if lid.startswith("_") or not isinstance(state, dict):
            continue
        if state.get("in_use") or float(state.get("epoch_available", 0.0) or 0.0) > farm.now_epoch():
            try:
                found.add(int(lid))
            except ValueError:
                pass
    return found


def choose_server_lid(
    commander_state: dict[str, Any],
    target_info: dict[str, Any],
    lords_response: dict[str, Any],
    lids: tuple[int, ...],
) -> int | None:
    target_lids = target_available_lids(target_info)
    if not target_lids:
        return None
    global_available = global_lids(lords_response)
    excluded = active_lids(commander_state)
    allowed = set(lids)
    for lid in lids:
        if lid in allowed and lid in target_lids and lid in global_available and lid not in excluded:
            return lid
    return None


def get_lords_fresh(socket, commander_state: dict[str, Any]) -> dict[str, Any]:
    request_wait(commander_state)
    farm.mark_request_sent(commander_state)
    try:
        return socket.get_lords()
    except Exception as exc:
        if is_closed_connection_error(exc):
            raise SessionClosed(str(exc)) from exc
        raise


def adi_cooldown_until(target_info: dict[str, Any]) -> float | None:
    data = target_info.get("payload", {}).get("data") if isinstance(target_info, dict) else {}
    if not isinstance(data, dict):
        return None
    candidates = []
    for key in ("cd", "cooldown", "CD", "COOLDOWN"):
        value = data.get(key)
        if value in (None, False, 0, "0"):
            continue
        try:
            candidates.append(float(value))
        except (TypeError, ValueError):
            return farm.now_epoch() + farm.random_between(TARGET_ADI_COOLDOWN_FALLBACK_RANGE)
    if not candidates:
        return None
    value = max(candidates)
    if value > farm.MIN_REAL_EPOCH:
        return value + farm.random_between(farm.RETURN_RANDOM_HOLD_RANGE)
    return farm.now_epoch() + max(value, farm.random_between(TARGET_ADI_COOLDOWN_FALLBACK_RANGE))


def adi_unavailable_reason(target_info: dict[str, Any]) -> str | None:
    if not isinstance(target_info, dict):
        return "adi_no_response"
    payload = target_info.get("payload", {})
    status = payload.get("status")
    if status not in (0, "0", None):
        return f"adi_status_{status}"
    data = payload.get("data")
    if not isinstance(data, dict):
        return "adi_no_data"
    if adi_cooldown_until(target_info) is not None:
        return "adi_cooldown"
    if not target_available_lids(target_info):
        return "adi_no_target_lids"
    return None


def hold_unavailable_target(
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any],
    key: str,
    rbc: dict[str, Any],
    *,
    reason: str,
    cooldown_until: float | None = None,
) -> None:
    retry_at = cooldown_until
    if retry_at is None:
        retry_at = farm.now_epoch() + farm.random_between(TARGET_ADI_UNAVAILABLE_RETRY_RANGE)
    rbc.update(
        {
            "epoch_available": retry_at,
            "pending_adi": False,
            "pending_cra_after": 0.0,
            "last_status": reason,
            "last_error": reason,
        }
    )
    rbc_state[key] = rbc
    log(f"skip_cra target={key} reason={reason} retry_at={int(retry_at)}")
    save_states(commander_state, rbc_state)


def refresh_target_info(
    socket,
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any],
    key: str,
    rbc: dict[str, Any],
) -> dict[str, Any] | None:
    x = rbc.get("x")
    y = rbc.get("y")
    if x is None or y is None:
        x, y = farm.parse_xy_key(key)
    if x is None or y is None:
        return None
    request_wait(commander_state)
    farm.mark_request_sent(commander_state)
    try:
        response = socket.get_target_infos(BURNING_SANDS, farm.SOURCE_X, farm.SOURCE_Y, int(x), int(y), quiet=False)
    except Exception as exc:
        if is_closed_connection_error(exc):
            raise SessionClosed(str(exc)) from exc
        hold_unavailable_target(
            commander_state,
            rbc_state,
            key,
            rbc,
            reason=f"adi_exception_{type(exc).__name__}",
        )
        return None
    parsed = normalize_response("adi", response)
    farm.process_adi(parsed, rbc_state)
    fresh = rbc_state.get(key)
    if isinstance(fresh, dict):
        rbc.update(fresh)
    cooldown_until = adi_cooldown_until(response)
    unavailable_reason = adi_unavailable_reason(response)
    if unavailable_reason is not None:
        hold_unavailable_target(
            commander_state,
            rbc_state,
            key,
            rbc,
            reason=unavailable_reason,
            cooldown_until=cooldown_until,
        )
        return None
    available = sorted(target_available_lids(response))
    rbc["last_adi_available_lids"] = available
    log(f"adi target={key} level={rbc.get('level')} available_lids={','.join(map(str, available)) if available else '-'}")
    return response


def movement_lord_ids(response: dict | bool) -> set[int]:
    if not isinstance(response, dict):
        return set()
    found: set[int] = set()

    def walk(value: Any) -> None:
        if isinstance(value, dict):
            for key, item in value.items():
                if key in {"LID", "lord_id"}:
                    try:
                        found.add(int(item))
                    except (TypeError, ValueError):
                        pass
                else:
                    walk(item)
        elif isinstance(value, list):
            for item in value:
                walk(item)

    walk(response.get("payload", {}).get("data"))
    return found


def refresh_server_movements(socket, commander_state: dict[str, Any]) -> bool:
    meta = commander_state.setdefault("_meta", {})
    now = farm.now_epoch()
    if float(meta.get("next_pygge_movement_refresh_epoch", 0.0) or 0.0) > now:
        return False
    meta["next_pygge_movement_refresh_epoch"] = now + farm.random_between(MOVEMENT_REFRESH_RANGE)
    response = socket.get_movements(quiet=True)
    server_lids = movement_lord_ids(response)
    if not server_lids:
        log("movements active_lids=unknown")
        return False
    changed = False
    for lid in server_lids:
        lid_key = str(lid)
        state = commander_state.get(lid_key)
        if not isinstance(state, dict):
            continue
        hold_until = now + farm.random_between(SERVER_ACTIVE_LID_HOLD_RANGE)
        if float(state.get("epoch_available", 0.0) or 0.0) < hold_until:
            state["epoch_available"] = hold_until
            changed = True
        if not state.get("in_use"):
            state["in_use"] = True
            state["status"] = "outbound"
            changed = True
        state["last_update"] = now
    meta["last_server_movement_lids"] = sorted(server_lids)
    log(f"movements active_lids={','.join(str(lid) for lid in sorted(server_lids))}")
    return changed


def recent_cra_rows(commander_state: dict[str, Any]) -> list[dict[str, Any]]:
    rows = commander_state.setdefault("_meta", {}).setdefault("pygge_cra_results", [])
    return rows if isinstance(rows, list) else []


def record_cra_result(
    commander_state: dict[str, Any],
    *,
    ok: bool,
    target: str,
    lid: int,
    level: int | None,
    code: str | int,
) -> None:
    meta = commander_state.setdefault("_meta", {})
    rows = recent_cra_rows(commander_state)
    rows.append(
        {
            "epoch": farm.now_epoch(),
            "ok": bool(ok),
            "target": target,
            "lid": int(lid),
            "level": level,
            "code": str(code),
        }
    )
    del rows[:-max(MAX_DIRECT_CRA_PER_SESSION, CRA_RESULT_WINDOW_SIZE)]
    meta["pygge_cra_results"] = rows
    meta["pygge_consecutive_cra_errors"] = 0 if ok else int(meta.get("pygge_consecutive_cra_errors", 0) or 0) + 1


def cra_count_since(commander_state: dict[str, Any], seconds: float) -> int:
    cutoff = farm.now_epoch() - seconds
    return sum(1 for row in recent_cra_rows(commander_state) if float(row.get("epoch", 0.0) or 0.0) >= cutoff)


def safety_stop_reason(commander_state: dict[str, Any]) -> str | None:
    meta = commander_state.setdefault("_meta", {})
    rows = recent_cra_rows(commander_state)
    session_start = float(meta.get("pygge_session_start_epoch", farm.now_epoch()) or farm.now_epoch())
    session_rows = [
        row for row in rows if float(row.get("epoch", 0.0) or 0.0) >= session_start
    ]
    if MAX_DIRECT_CRA_PER_SESSION and len(session_rows) >= MAX_DIRECT_CRA_PER_SESSION:
        return f"session_cra_cap count={len(session_rows)}"
    last_hour = cra_count_since(commander_state, 3600.0)
    if MAX_DIRECT_CRA_PER_HOUR and last_hour >= MAX_DIRECT_CRA_PER_HOUR:
        return f"hourly_cra_cap count={last_hour}"
    window = rows[-CRA_RESULT_WINDOW_SIZE:]
    if len(window) >= CRA_RESULT_WINDOW_SIZE:
        errors = sum(not bool(row.get("ok")) for row in window)
        if errors >= CRA_RESULT_WINDOW_STOP_ERRORS:
            return f"error_window errors={errors}/{len(window)}"
    consecutive = int(meta.get("pygge_consecutive_cra_errors", 0) or 0)
    if consecutive >= CONSECUTIVE_CRA_ERROR_STOP:
        return f"consecutive_cra_errors count={consecutive}"
    return None


def begin_safety_session() -> None:
    commander_state = farm.load_commander_state()
    meta = commander_state.setdefault("_meta", {})
    now = farm.now_epoch()
    rows = recent_cra_rows(commander_state)
    meta["pygge_cra_results"] = [
        row for row in rows if float(row.get("epoch", 0.0) or 0.0) >= now - 6 * 3600
    ]
    meta["pygge_session_start_epoch"] = now
    meta["pygge_consecutive_cra_errors"] = 0
    farm.save_json(farm.COMMANDER_STATE_PATH, commander_state)


def extend_ack_return_from_response(
    response: dict[str, Any],
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any],
    *,
    lid: int,
    target: str,
) -> None:
    movement = (
        response.get("payload", {})
        .get("data", {})
        .get("AAM", {})
        .get("M", {})
    )
    if not isinstance(movement, dict):
        return
    try:
        travel = float(movement.get("TT", 0.0) or 0.0)
    except (TypeError, ValueError):
        travel = 0.0
    if travel <= 0:
        return
    return_at = (
        farm.now_epoch()
        + travel * farm.random_between(ACK_RETURN_MULTIPLIER_RANGE)
        + farm.random_between(ACK_RETURN_HOLD_RANGE)
    )
    state = commander_state.get(str(lid))
    if isinstance(state, dict) and float(state.get("epoch_available", 0.0) or 0.0) < return_at:
        state["epoch_available"] = return_at
        state["backup_return"] = True
        state["last_update"] = farm.now_epoch()
    rbc = rbc_state.get(target)
    if isinstance(rbc, dict):
        rbc["last_return_fallback_epoch"] = return_at


def send_gaa(socket, commander_state: dict[str, Any], rbc_state: dict[str, Any], *, radius: int) -> tuple[int, int]:
    request_wait(commander_state)
    ax1, ay1 = farm.next_scan_chunk(commander_state, radius=radius)
    farm.mark_request_sent(commander_state)
    response = socket.get_map_chunk(BURNING_SANDS, ax1, ay1, quiet=False)
    parsed = normalize_response("gaa", response)
    changed = farm.process_gaa(parsed, rbc_state)
    log(f"gaa chunk={ax1}:{ay1} changed={changed} rbcs={len([k for k in rbc_state if not k.startswith('_')])}")
    return ax1, ay1


def send_adi(socket, commander_state: dict[str, Any], rbc_state: dict[str, Any], key: str, rbc: dict[str, Any]) -> bool:
    x = rbc.get("x")
    y = rbc.get("y")
    if x is None or y is None:
        x, y = farm.parse_xy_key(key)
    if x is None or y is None:
        return False
    request_wait(commander_state)
    farm.mark_request_sent(commander_state)
    try:
        response = socket.get_target_infos(BURNING_SANDS, farm.SOURCE_X, farm.SOURCE_Y, int(x), int(y), quiet=False)
    except Exception as exc:
        if is_closed_connection_error(exc):
            raise SessionClosed(str(exc)) from exc
        hold_unavailable_target(
            commander_state,
            rbc_state,
            key,
            rbc,
            reason=f"adi_exception_{type(exc).__name__}",
        )
        return True
    parsed = normalize_response("adi", response)
    changed = farm.process_adi(parsed, rbc_state)
    available = sorted(target_available_lids(response))
    current = rbc_state.get(key)
    if isinstance(current, dict):
        current["last_adi_available_lids"] = available
    cooldown_until = adi_cooldown_until(response)
    unavailable_reason = adi_unavailable_reason(response)
    if unavailable_reason is not None and isinstance(current, dict):
        hold_unavailable_target(
            commander_state,
            rbc_state,
            key,
            current,
            reason=unavailable_reason,
            cooldown_until=cooldown_until,
        )
        return True
    log(f"adi target={key} level={rbc_state.get(key, {}).get('level')} changed={changed}")
    return changed


def build_attack_payload(tx: int, ty: int, lid: int, attack: farm.Attack) -> dict[str, Any]:
    return {
        "SX": farm.SOURCE_X,
        "SY": farm.SOURCE_Y,
        "TX": int(tx),
        "TY": int(ty),
        "KID": farm.KINGDOM_ID,
        "LID": int(lid),
        "WT": 0,
        "HBW": farm.HBW_VALUE,
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


def send_cra(
    socket,
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any],
    key: str,
    rbc: dict[str, Any],
    lid: int,
    attack: farm.Attack,
) -> bool:
    x = rbc.get("x")
    y = rbc.get("y")
    if x is None or y is None:
        x, y = farm.parse_xy_key(key)
    if x is None or y is None:
        return False
    attack_name = "LEVEL_61" if attack is farm.LEVEL_61 else "LOW_50" if attack is farm.NOT_LV_50_AND_BELOW else "NOT_LV_61"
    request_wait(commander_state)
    attack_wait(commander_state)
    now = farm.now_epoch()
    commander_state[str(lid)].update(
        {
            "epoch_available": now + farm.BACKUP_LID_RETURN_SECONDS + farm.random_between(farm.RETURN_RANDOM_HOLD_RANGE),
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
            "last_status": "pending_cra",
        }
    )
    save_states(commander_state, rbc_state)
    payload = build_attack_payload(int(x), int(y), lid, attack)
    farm.mark_request_sent(commander_state)
    mark_attack_sent(commander_state)
    try:
        socket.send_json_command("cra", payload)
        response = socket.wait_for_json_response("cra")
    except Exception as exc:
        parsed = {"command": "cra", "status": "exception", "payload": None, "epoch": farm.now_epoch()}
        farm.process_cra_error(parsed, commander_state, rbc_state)
        retry_at = mark_attack_error_cooldown(commander_state)
        record_cra_result(commander_state, ok=False, target=key, lid=lid, level=rbc.get("level"), code="exception")
        log(f"cra_error target={key} level={rbc.get('level')} lid={lid} code=exception retry_at={int(retry_at)} error={str(exc)!r}", error=True)
        return True
    parsed = normalize_response("cra", response)
    if parsed.get("status") not in (0, "0", None):
        farm.process_cra_error(parsed, commander_state, rbc_state)
        retry_at = mark_attack_error_cooldown(commander_state)
        record_cra_result(commander_state, ok=False, target=key, lid=lid, level=rbc.get("level"), code=parsed.get("status"))
        log(f"cra_error target={key} level={rbc.get('level')} lid={lid} code={parsed.get('status')} retry_at={int(retry_at)} error=server_rejected", error=True)
        return True
    changed = farm.process_cra_ack(parsed, commander_state, rbc_state)
    extend_ack_return_from_response(response, commander_state, rbc_state, lid=lid, target=key)
    record_cra_result(commander_state, ok=True, target=key, lid=lid, level=rbc.get("level"), code=0)
    log(f"cra target={key} level={rbc.get('level')} lid={lid} attack={attack_name} status=0 changed={changed} {active_summary(commander_state)}")
    return True


def target_needs_adi_refresh(rbc: dict[str, Any], now: float) -> bool:
    last_adi_epoch = float(rbc.get("last_adi_epoch", 0.0) or 0.0)
    stale_adi = now - last_adi_epoch > farm.ADI_REFRESH_AFTER_SECONDS
    random_refresh = (
        now - last_adi_epoch > farm.ADI_REFRESH_AFTER_SECONDS / 3
        and random.random() < farm.ADI_REFRESH_PROBABILITY
    )
    return stale_adi or random_refresh


def direct_attack_for_level(level: int | None) -> farm.Attack | None:
    if level is None:
        return None
    level = int(level)
    if level == farm.LV61 and random.random() >= LV61_CROSSBOW_ATTACK_PROBABILITY:
        return farm.NOT_LV_61
    return farm.attack_for_level(level)


def direct_queue_sort_key(queue_name: str, item: tuple[str, dict[str, Any]]) -> tuple[float, float, float]:
    _, rbc = item
    now = farm.now_epoch()
    epoch_available = farm.clean_epoch(rbc.get("epoch_available"))
    pending_cra_after = farm.clean_epoch(rbc.get("pending_cra_after"))
    wait_remaining = max(0.0, max(epoch_available, pending_cra_after) - now)
    distance = float(rbc.get("distance") or 0.0)
    if queue_name == "non61":
        level = int(rbc.get("level") or 0)
        edge_distance = abs(distance - NON61_EDGE_DISTANCE_TARGET) + random.uniform(-1.8, 1.8)
        return wait_remaining, -level, edge_distance, random.random()
    return wait_remaining, distance + random.uniform(-2.75, 2.75), random.random()


def partition_target_queues(
    rbc_state: dict[str, Any],
    now: float,
) -> tuple[list[tuple[str, dict[str, Any]]], list[tuple[str, dict[str, Any]]], list[tuple[str, dict[str, Any]]]]:
    unknown: list[tuple[str, dict[str, Any]]] = []
    lv61: list[tuple[str, dict[str, Any]]] = []
    non61: list[tuple[str, dict[str, Any]]] = []
    for key, rbc in farm.eligible_targets(rbc_state):
        if rbc.get("under_attack"):
            continue
        if float(rbc.get("epoch_available", 0.0) or 0.0) > now:
            continue
        level = rbc.get("level")
        if level is None:
            unknown.append((key, rbc))
            continue
        if int(level) == farm.LV61:
            lv61.append((key, rbc))
        else:
            non61.append((key, rbc))
    lv61.sort(key=lambda item: direct_queue_sort_key("lv61", item))
    non61.sort(key=lambda item: direct_queue_sort_key("non61", item))
    unknown.sort(key=lambda item: direct_queue_sort_key("unknown", item))
    return lv61, non61, unknown


def send_from_queue(
    socket,
    commander_state: dict[str, Any],
    rbc_state: dict[str, Any],
    queue_name: str,
    targets: list[tuple[str, dict[str, Any]]],
    lids: tuple[int, ...],
) -> bool:
    if queue_is_paused(commander_state, queue_name):
        log(f"queue_skip queue={queue_name} reason=queue_pause")
        return False
    if not pool_has_ready_lid(commander_state, lids):
        log(f"queue_skip queue={queue_name} reason=local_lid_pool_exhausted")
        return False
    for key, rbc in targets:
        level = int(rbc.get("level"))
        attack = direct_attack_for_level(level)
        if attack is None:
            continue
        target_info = refresh_target_info(socket, commander_state, rbc_state, key, rbc)
        if target_info is None:
            continue
        lords_response = get_lords_fresh(socket, commander_state)
        lid = choose_server_lid(commander_state, target_info, lords_response, lids)
        if lid is None:
            retry_range = farm.NO_LV61_LID_RETRY_RANGE if queue_name == "lv61" else TARGET_LID_UNAVAILABLE_RETRY_RANGE
            queue_retry_at = None
            if queue_name == "lv61":
                queue_retry_at = pause_queue(commander_state, queue_name, LV61_QUEUE_PAUSE_AFTER_NO_LID_RANGE)
            rbc["epoch_available"] = farm.now_epoch() + farm.random_between(retry_range)
            rbc["last_status"] = f"no_server_lid_for_{queue_name}"
            log(
                f"no_server_lid queue={queue_name} target={key} level={level} "
                f"target_lids={','.join(map(str, sorted(target_available_lids(target_info)))) or '-'} "
                f"retry_at={int(rbc['epoch_available'])}"
                f"{f' queue_retry_at={int(queue_retry_at)}' if queue_retry_at is not None else ''}"
            )
            save_states(commander_state, rbc_state)
            return False
        return send_cra(socket, commander_state, rbc_state, key, rbc, lid, attack)
    return False


def schedule_direct(socket, commander_state: dict[str, Any], rbc_state: dict[str, Any]) -> bool:
    now = farm.now_epoch()
    stop_reason = safety_stop_reason(commander_state)
    if stop_reason is not None:
        log(f"safety_stop reason={stop_reason}", error=True)
        raise SafetyStop(stop_reason)
    active = active_count(commander_state)
    if active >= MAX_ACTIVE_DIRECT:
        log(f"active_cap wait active={active}/{MAX_ACTIVE_DIRECT} {active_summary(commander_state)}")
        return False
    cooldown = farm.request_cooldown_remaining(commander_state, now=now)
    if cooldown > 0:
        log(f"blocked cooldown={cooldown:.1f}s {active_summary(commander_state)}")
        return False
    pending_lid, pending_state = farm.newest_pending_cra(commander_state)
    lv61_targets, non61_targets, unknown_targets = partition_target_queues(rbc_state, now)
    if pending_lid is not None:
        log(f"blocked pending_ack lid={pending_lid} target={pending_state.get('target') if pending_state else None}")
        return False
    if send_from_queue(socket, commander_state, rbc_state, "lv61", lv61_targets, LV61_COMMANDER_LIDS):
        return True
    if send_from_queue(socket, commander_state, rbc_state, "non61", non61_targets, NON_LV61_COMMANDER_LIDS):
        return True
    for key, rbc in unknown_targets:
        rbc["pending_adi"] = True
        rbc["pending_cra_after"] = 0.0
        rbc["epoch_available"] = now + farm.random_between(farm.UNKNOWN_ADI_RETRY_RANGE)
        rbc["last_status"] = "waiting_for_adi_level"
        return send_adi(socket, commander_state, rbc_state, key, rbc)
    log(f"idle no_target {active_summary(commander_state)}")
    return False


def run_setup_scan(socket, count: int, *, radius: int) -> None:
    commander_state = farm.load_commander_state()
    rbc_state = farm.load_rbc_state()
    for index in range(max(0, count)):
        send_gaa(socket, commander_state, rbc_state, radius=radius)
        save_states(commander_state, rbc_state)
        log(f"setup_scan progress={index + 1}/{count}")
        time.sleep(farm.random_between(SCAN_CHUNK_DELAY_RANGE))
    save_states(commander_state, rbc_state)


def record_error(commander_state: dict[str, Any]) -> float:
    meta = commander_state.setdefault("_meta", {})
    recent = meta.setdefault("recent_pygge_errors", [])
    recent.append(farm.now_epoch())
    del recent[:-MAX_RECENT_ERRORS]
    failures = min(8, int(meta.get("pygge_consecutive_errors", 0) or 0) + 1)
    meta["pygge_consecutive_errors"] = failures
    if len(recent) >= ERROR_PAUSE_TRIGGER:
        recent.clear()
        pause = farm.random_between(GLOBAL_ERROR_PAUSE_RANGE)
        meta["pygge_global_pause_until"] = farm.now_epoch() + pause
        return pause
    raw = min(ERROR_BACKOFF_MAX_SECONDS, ERROR_BACKOFF_BASE_SECONDS * (2 ** (failures - 1)))
    return raw * random.uniform(0.85, 1.25)


def record_success(commander_state: dict[str, Any]) -> None:
    meta = commander_state.setdefault("_meta", {})
    meta["pygge_consecutive_errors"] = 0


def run_loop(socket, *, sleep_seconds: float, timeout_minutes: float, scan_radius: int, once: bool) -> None:
    started_at = farm.now_epoch()
    timeout_seconds = max(0.0, timeout_minutes) * 60.0
    while True:
        if timeout_seconds and farm.now_epoch() - started_at >= timeout_seconds:
            log(f"timeout_stop minutes={timeout_minutes:.2f}")
            return
        commander_state = farm.load_commander_state()
        rbc_state = farm.load_rbc_state()
        pause_until = float(commander_state.setdefault("_meta", {}).get("pygge_global_pause_until", 0.0) or 0.0)
        if pause_until > farm.now_epoch():
            wait = pause_until - farm.now_epoch()
            log(f"global_pause wait={wait:.1f}s", error=True)
            time.sleep(wait)
            continue
        changed = refresh_server_movements(socket, commander_state)
        changed = farm.release_stale_pending_cra(commander_state, rbc_state) or changed
        changed = farm.reconcile_active_state(commander_state, rbc_state) or changed
        changed = farm.release_returned_commanders(commander_state, rbc_state) or changed
        changed = farm.reconcile_active_state(commander_state, rbc_state) or changed
        try:
            action = schedule_direct(socket, commander_state, rbc_state)
            if not action and farm.AUTO_REFRESH_SCAN:
                send_gaa(socket, commander_state, rbc_state, radius=scan_radius)
                action = True
            record_success(commander_state)
            changed = changed or action
        except SafetyStop:
            save_states(commander_state, rbc_state)
            raise
        except TimeoutError as exc:
            if "Connection is already closed" in str(exc):
                raise SessionClosed(str(exc)) from exc
            delay = record_error(commander_state)
            log(f"timeout backoff={delay:.1f}s error={str(exc)!r}", error=True)
            changed = True
            save_states(commander_state, rbc_state)
            time.sleep(delay)
            continue
        except Exception as exc:
            if "Connection is already closed" in str(exc):
                raise SessionClosed(str(exc)) from exc
            delay = record_error(commander_state)
            log(f"error backoff={delay:.1f}s error={str(exc)!r}", error=True)
            changed = True
            save_states(commander_state, rbc_state)
            time.sleep(delay)
            continue
        if changed:
            save_states(commander_state, rbc_state)
        if once:
            return
        gap = max(sleep_seconds, farm.STRICT_ATTACK_PACKET_COOLDOWN_MIN) + random.uniform(0.0, 5.0)
        if not changed:
            gap = random.uniform(*IDLE_SLEEP_RANGE)
        time.sleep(gap)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Direct pygge Burning Sands RBC farmer.")
    parser.add_argument("--account-config", type=Path, default=DEFAULT_ACCOUNT)
    parser.add_argument("--setup-scan", action="store_true")
    parser.add_argument("--scan-count", type=int, default=12)
    parser.add_argument("--scan-radius", type=int, default=farm.MAP_SCAN_RADIUS)
    parser.add_argument("--sleep", type=float, default=farm.STRICT_ATTACK_PACKET_COOLDOWN_MIN)
    parser.add_argument("--timeout-minutes", type=float, default=farm.REQUEST_SEQUENCE_TIMEOUT_MINUTES)
    parser.add_argument("--once", action="store_true", help="run one direct scheduling pass after login")
    parser.add_argument("--log-dir", type=Path, default=DEFAULT_LOG_DIR)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    global LOG_FILE
    args = parse_args(argv)
    args.log_dir.mkdir(parents=True, exist_ok=True)
    LOG_FILE = (args.log_dir / datetime.now().strftime("sand_pygge_%Y%m%d_%H%M%S.log")).open("a", encoding="utf-8")
    socket = None
    reconnect_failures = 0
    try:
        log(
            "starting "
            f"account_config={args.account_config} setup_scan={args.setup_scan} "
            f"scan_count={args.scan_count} timeout_min={args.timeout_minutes} once={args.once}"
        )
        begin_safety_session()
        socket, _, _, _ = connect_sources_with_cooldown(args.account_config)
        reconnect_failures = 0
        if args.setup_scan:
            run_setup_scan(socket, args.scan_count, radius=args.scan_radius)
            return 0
        while True:
            try:
                if socket_is_closed(socket):
                    raise SessionClosed("socket closed")
                run_loop(
                    socket,
                    sleep_seconds=args.sleep,
                    timeout_minutes=args.timeout_minutes,
                    scan_radius=args.scan_radius,
                    once=args.once,
                )
                return 0
            except SessionClosed as exc:
                log(f"session_closed error={str(exc)!r}", error=True)
                socket, _, _, _, reconnect_failures = reconnect_sources(socket, args.account_config, reconnect_failures)
            except SafetyStop as exc:
                log(f"stopped_safely reason={str(exc)!r}", error=True)
                return 0
    except KeyboardInterrupt:
        log("interrupted")
        return 130
    finally:
        if socket is not None:
            disconnect(socket)
        if LOG_FILE is not None:
            LOG_FILE.close()
            LOG_FILE = None


if __name__ == "__main__":
    raise SystemExit(main())
