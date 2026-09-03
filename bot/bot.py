
from __future__ import annotations

import argparse
import importlib
import json
import math
import os
import random
import sys
import time
from datetime import datetime
from pathlib import Path
from typing import Any

import psycopg
from psycopg.types.json import Jsonb


def find_repo_root(path: Path) -> Path:
    for parent in (path, *path.parents):
        if (parent / "bot" / "scheduler.py").is_file():
            return parent
    return path.parents[2]


REPO_ROOT = find_repo_root(Path(__file__).resolve())
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

SCAN_ROOT = Path(os.environ.get("GGE_SCAN_ROOT", "/Users/edisonhussey/Desktop/scan_coordinates"))
for path in (SCAN_ROOT, SCAN_ROOT / "pygge_repo"):
    if str(path) not in sys.path:
        sys.path.insert(0, str(path))

from bot.test_psql_connection import connect, read_connection_config
from bot import account_context
from bot import berimond
from bot import storm_database as storm_db
from bot.db_account import DEFAULT_BACKFILL_AID, ensure_account_columns
from bot.storm_database import (
    STORM_KID,
    cleanup_expired_storm_targets,
    ensure_storm_tables,
    release_storm_target,
    reserve_storm_target,
)
from bot.sand_rbc_farm import main as farm

try:
    from event_worker.castles import find_castle_xy
    from event_worker.gge_session import LoginTemporarilyBlocked, connect_and_login, disconnect
    from event_worker.kingdoms import BURNING_SANDS
except ImportError:
    find_castle_xy = None
    connect_and_login = None
    disconnect = None
    LoginTemporarilyBlocked = RuntimeError
    BURNING_SANDS = 1


# //set out the attack return time to be equal to travel duration + return duration initlaly heusrtic value of 20mins
# //once fixed and read the land valuefrom the logs, you can adjust the database to reduce if it in fact going to be freer earlier. 

# /// use the function 4.4 + 2**(rand(1,4.5)) as the increase anti bot cooldown after theoretical cooldown
# ///base increase is 3 hours + time once landed, else use heuristic 20 mins + 3 hours from sent. whichever is lower. then apply the randomization funciton outlines. 

# //read sand_rbc_farm it works well. we want to create 2 version one with debugging mitprox, and another version that uses the log in timeout structure from pygge
# but the underlying core and event sequencing, of queue for requests / processing .

# //only apply the sand rbc 50 crosswbomen cra packet for first 13 commander count. on left flank and once sent update the datbase to last_attacked. use this same field to deteremine later on if it can be attacked. strict 20 sec + random 0-10 timeout between request intervals. also need an adi before cra with also randomized, anti bot detection. 

# /// and need to simulate opening sand kingdom once open, and nothing at all should seem robotic, and commanders should persist between session if you need to create  a new table .


# the inject vs pygge is just a wrapper on the core. and we done a loto fthe work before we like to use the datbase need kid =1 and current level 61 only . 

# //keep these comments just now write bot.py and proxy_bot.py


HERE = Path(__file__).resolve().parent
BOT_STATE_DIR = REPO_ROOT / "bot"
DEFAULT_ACCOUNT = SCAN_ROOT / "ventrilo.ini"
DEFAULT_LOG_DIR = BOT_STATE_DIR / "logs"
CONTROL_FILE = BOT_STATE_DIR / "proxy_control.json"
CURRENT_ACCOUNT_NAME = "ventrilo"
CURRENT_AID = DEFAULT_BACKFILL_AID
SANDS_KID = 1
TARGET_LEVEL = 61
FIRST_13_COMMANDER_LIDS = (0, 2, 3, 6, 7, 8, 9, 10, 11, 16, 17, 18, 20, 21, 22)
REQUEST_INTERVAL_RANGE = (20.0, 30.0)
ADI_TO_CRA_DELAY_RANGE = (5.5, 13.0)
IDLE_SLEEP_RANGE = (55.0, 145.0)
TARGET_RESERVE_SECONDS = 12 * 60
TARGET_NO_LID_RETRY_RANGE = (18 * 60.0, 44 * 60.0)
TARGET_BAD_LEVEL_RETRY_RANGE = (2.5 * 3600.0, 4.0 * 3600.0)
TARGET_ERROR_RETRY_RANGE = (21 * 60.0, 53 * 60.0)
HEURISTIC_RETURN_SECONDS = 30 * 60
RETURN_HEURISTIC_MULTIPLIER = 0.3
COMMANDER_RETURN_HOLD_RANGE = (45.0, 180.0)
MAX_CRA_PER_HOUR_DEFAULT = 3
MAX_CONSECUTIVE_ERRORS = 2
MAX_STORM_CONSECUTIVE_CRA_REJECTS = 2
STORM_SCAN_INTERVAL_RANGE = (5.5, 12.5)
STORM_SCAN_RADIUS = 96
STORM_TARGET_FRESH_SECONDS = 120
STORM_SOURCE_X = 675
STORM_SOURCE_Y = 675
STORM_HBW = -1
STORM_PTT = 1
BERIMOND_KID = berimond.KID
BERIMOND_SOURCE_X = berimond.SOURCE_X
BERIMOND_SOURCE_Y = berimond.SOURCE_Y
BERIMOND_TARGET_X = berimond.TARGET_X
BERIMOND_TARGET_Y = berimond.TARGET_Y
BERIMOND_HBW = berimond.HBW
BERIMOND_PTT = berimond.PTT
BERIMOND_AV = berimond.AV
BERIMOND_COMMANDER_COUNT = berimond.COMMANDER_COUNT
BERIMOND_GLOBAL_ATTACK_COOLDOWN_SECONDS = berimond.GLOBAL_ATTACK_COOLDOWN_SECONDS
BERIMOND_MAX_CRA_ERRORS = berimond.MAX_CRA_ERRORS
BERIMOND_CRA_ERROR_WINDOW_SECONDS = berimond.CRA_ERROR_WINDOW_SECONDS
BERIMOND_MAX_COMMANDER_OUT_SECONDS = berimond.MAX_COMMANDER_OUT_SECONDS
BERIMOND_ALERT_ON_ERROR = berimond.ALERT_ON_ERROR
BERIMOND_ALERT_SOUND_PATH = berimond.ALERT_SOUND_PATH
PROXY_PENDING_TIMEOUT = 45.0


def known_commander_lids() -> tuple[int, ...]:
    commander_lids = set(FIRST_13_COMMANDER_LIDS)
    try:
        commander_lids.update(task_scheduler.commander_range(1, BERIMOND_COMMANDER_COUNT))
    except Exception:
        pass
    for task in getattr(task_scheduler, "TASKS", ()):
        commander_lids.update(int(lid) for lid in getattr(task, "commander_lids", ()) or ())
    return tuple(sorted(commander_lids))
PROXY_ADI_TO_CRA_TIMEOUT = 110.0
PROXY_CRA_TIMEOUT = 135.0
PROXY_CRA_TIMEOUT_GRACE = 8.0
LOG_FILE = None
task_scheduler = importlib.import_module("bot.scheduler")


def current_aid() -> str:
    return str(CURRENT_AID)


def configure_account(account_name: str | None = None, aid: str | None = None):
    global CURRENT_ACCOUNT_NAME, CURRENT_AID, BOT_STATE_DIR, DEFAULT_LOG_DIR, CONTROL_FILE
    if account_name:
        context = account_context.for_account_name(account_name)
    elif aid:
        context = account_context.for_aid(str(aid), CURRENT_ACCOUNT_NAME)
    else:
        context = account_context.for_aid(CURRENT_AID, CURRENT_ACCOUNT_NAME)
    CURRENT_ACCOUNT_NAME = context.username or CURRENT_ACCOUNT_NAME
    CURRENT_AID = context.aid
    BOT_STATE_DIR = context.root
    DEFAULT_LOG_DIR = context.logs_dir
    CONTROL_FILE = context.control_file
    farm.COMMANDER_STATE_PATH = context.gamestate_dir / "commander_state.json"
    farm.RBC_STATE_PATH = context.gamestate_dir / "rbc_state.json"
    farm.LATEST_SERVER_MESSAGES = context.latest_logs_dir
    storm_db.set_account_aid(context.aid)
    return context


class SessionClosed(RuntimeError):
    pass


class SafetyStop(RuntimeError):
    pass


def now_epoch() -> int:
    return int(time.time())


def log(message: str, *, error: bool = False) -> None:
    stamp = datetime.now().astimezone().isoformat(timespec="seconds")
    line = f"[empire-bot] {'ERROR' if error else 'INFO'} {message} ts={stamp}"
    print(line, file=sys.stderr if error else sys.stdout, flush=True)
    if LOG_FILE is not None:
        print(line, file=LOG_FILE, flush=True)


def cooldown_extra_seconds() -> float:
    return 4.4 + 2 ** random.uniform(1.0, 4.5)


def estimated_return_seconds(outbound_seconds: float | int | None) -> int:
    try:
        outbound = float(outbound_seconds or 0)
    except (TypeError, ValueError):
        outbound = 0.0
    if outbound > 0:
        return int(max(0.0, outbound * RETURN_HEURISTIC_MULTIPLIER))
    return HEURISTIC_RETURN_SECONDS


def ensure_bot_tables(conn: psycopg.Connection) -> None:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS commander_state (
                lord_id INTEGER PRIMARY KEY,
                available_after BIGINT NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'available',
                march_id BIGINT,
                target_rbc_id BIGINT,
                updated_at BIGINT NOT NULL DEFAULT 0
            )
            """
        )
        cur.execute(
            """
            CREATE TABLE IF NOT EXISTS bot_runtime_state (
                key TEXT PRIMARY KEY,
                value_text TEXT NOT NULL
            )
            """
        )
    conn.commit()
    ensure_account_columns(conn, aid)
    with conn.cursor() as cur:
        for lid in known_commander_lids():
            cur.execute(
                """
                INSERT INTO commander_state (aid, lord_id, available_after, status, updated_at)
                VALUES (%s, %s, 0, 'available', 0)
                ON CONFLICT (aid, lord_id) DO NOTHING
                """,
                (aid, lid),
            )
    conn.commit()


def ensure_commander_rows(conn: psycopg.Connection, lids: Iterable[int]) -> None:
    aid = current_aid()
    now = now_epoch()
    with conn.cursor() as cur:
        for lid in lids:
            cur.execute(
                """
                INSERT INTO commander_state (aid, lord_id, available_after, status, updated_at)
                VALUES (%s, %s, 0, 'available', %s)
                ON CONFLICT (aid, lord_id) DO NOTHING
                """,
                (aid, int(lid), now),
            )
    conn.commit()


def runtime_float(conn: psycopg.Connection, key: str, default: float = 0.0) -> float:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute("SELECT value_text FROM bot_runtime_state WHERE aid = %s AND key = %s", (aid, key))
        row = cur.fetchone()
    if row is None:
        return default
    try:
        return float(row[0])
    except (TypeError, ValueError):
        return default


def set_runtime_value(conn: psycopg.Connection, key: str, value: float | int | str) -> None:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO bot_runtime_state (aid, key, value_text)
            VALUES (%s, %s, %s)
            ON CONFLICT (aid, key) DO UPDATE SET value_text = EXCLUDED.value_text
            """,
            (aid, key, str(value)),
        )
    conn.commit()


def runtime_json(conn: psycopg.Connection, key: str) -> dict[str, Any] | None:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute("SELECT value_text FROM bot_runtime_state WHERE aid = %s AND key = %s", (aid, key))
        row = cur.fetchone()
    if row is None:
        return None
    try:
        value = json.loads(row[0])
    except (TypeError, json.JSONDecodeError):
        return None
    return value if isinstance(value, dict) else None


def set_runtime_json(conn: psycopg.Connection, key: str, value: dict[str, Any]) -> None:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO bot_runtime_state (aid, key, value_text)
            VALUES (%s, %s, %s)
            ON CONFLICT (aid, key) DO UPDATE SET value_text = EXCLUDED.value_text
            """,
            (aid, key, json.dumps(value, sort_keys=True)),
        )
    conn.commit()


def delete_runtime_value(conn: psycopg.Connection, key: str) -> None:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute("DELETE FROM bot_runtime_state WHERE aid = %s AND key = %s", (aid, key))
    conn.commit()


def persist_proxy_pending(conn: psycopg.Connection, pending: dict[str, Any]) -> None:
    set_runtime_json(conn, "proxy_pending", pending)


def load_proxy_pending_db(conn: psycopg.Connection) -> dict[str, Any] | None:
    return runtime_json(conn, "proxy_pending")


def clear_proxy_pending_db(conn: psycopg.Connection) -> None:
    delete_runtime_value(conn, "proxy_pending")


def persist_proxy_last_cra(conn: psycopg.Connection, last_cra: dict[str, Any]) -> None:
    set_runtime_json(conn, "proxy_last_cra", last_cra)


def load_proxy_last_cra_db(conn: psycopg.Connection) -> dict[str, Any] | None:
    return runtime_json(conn, "proxy_last_cra")


def clear_proxy_last_cra_db(conn: psycopg.Connection) -> None:
    delete_runtime_value(conn, "proxy_last_cra")


def clear_proxy_awaiting_db(conn: psycopg.Connection) -> None:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute("DELETE FROM bot_runtime_state WHERE aid = %s AND key IN ('proxy_pending', 'proxy_last_cra')", (aid,))
    conn.commit()


def load_proxy_control() -> dict[str, Any]:
    if not CONTROL_FILE.exists():
        return {}
    try:
        data = json.loads(CONTROL_FILE.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {}
    return data if isinstance(data, dict) else {}


def save_proxy_control(state: dict[str, Any]) -> None:
    CONTROL_FILE.parent.mkdir(parents=True, exist_ok=True)
    tmp = CONTROL_FILE.with_suffix(CONTROL_FILE.suffix + ".tmp")
    tmp.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    tmp.replace(CONTROL_FILE)


def stop_proxy_transport(reason: str) -> None:
    state = load_proxy_control()
    state["running"] = False
    state["pending"] = None
    state["stopped_at"] = now_epoch()
    state["stop_reason"] = reason
    save_proxy_control(state)
    try:
        with connect(read_connection_config()) as conn:
            ensure_bot_tables(conn)
            clear_proxy_awaiting_db(conn)
    except Exception as exc:
        log(f"proxy_db_clear_failed reason={reason} error={exc!r}", error=True)


def resume_proxy_transport_after_soft_reject(reason: str) -> None:
    state = load_proxy_control()
    state["running"] = True
    state["transport_only"] = True
    state["pending"] = None
    state["last_cra"] = None
    state["stopped_at"] = None
    state["stop_reason"] = None
    state["last_soft_reject"] = reason
    state["last_soft_reject_at"] = now_epoch()
    save_proxy_control(state)
    try:
        with connect(read_connection_config()) as conn:
            ensure_bot_tables(conn)
            clear_proxy_awaiting_db(conn)
    except Exception as exc:
        log(f"proxy_db_clear_failed reason=soft_reject_resume error={exc!r}", error=True)


def proxy_driver_stop_reason() -> str | None:
    state = load_proxy_control()
    if state.get("transport_only") and state.get("running") is False and state.get("stop_reason"):
        return str(state.get("stop_reason"))
    return None


def ensure_proxy_driver_running() -> None:
    reason = proxy_driver_stop_reason()
    if reason is not None:
        raise SafetyStop(f"proxy_driver_stopped reason={reason}")


def interruptible_proxy_sleep(seconds: float) -> None:
    deadline = time.time() + max(0.0, float(seconds))
    while time.time() < deadline:
        ensure_proxy_driver_running()
        remaining = deadline - time.time()
        if remaining <= 0:
            break
        time.sleep(min(0.5, remaining))


def wait_for_request_slot(conn: psycopg.Connection) -> None:
    next_epoch = runtime_float(conn, "next_request_epoch")
    wait = max(0.0, next_epoch - time.time())
    if wait > 0:
        log(f"request_wait seconds={wait:.1f}")
        time.sleep(wait)
    set_runtime_value(conn, "next_request_epoch", time.time() + random.uniform(*REQUEST_INTERVAL_RANGE))


def wait_between_adi_and_cra() -> None:
    delay = random.uniform(*ADI_TO_CRA_DELAY_RANGE)
    log(f"adi_to_cra_wait seconds={delay:.1f}")
    time.sleep(delay)


def connect_sands(account_config: Path):
    if connect_and_login is None or find_castle_xy is None:
        raise RuntimeError("event_worker is not importable; set GGE_SCAN_ROOT to the scan_coordinates folder")
    while True:
        try:
            socket = connect_and_login(config_path=account_config)
            castles = socket.get_castles()
            sx, sy, cid = find_castle_xy(castles, kingdom=BURNING_SANDS)
            socket.go_to_castle(BURNING_SANDS, cid if cid is not None else -1)
            socket.open_map(BURNING_SANDS)
            farm.SOURCE_X = int(sx)
            farm.SOURCE_Y = int(sy)
            log(f"connected_sands source={sx}:{sy} castle_id={cid}")
            return socket
        except LoginTemporarilyBlocked as exc:
            wait = max(float(getattr(exc, "retry_seconds", 0.0) or 0.0), 60.0) + random.uniform(45.0, 120.0)
            log(f"login_blocked wait={wait:.1f}", error=True)
            time.sleep(wait)


def response_data(response: dict[str, Any]) -> dict[str, Any]:
    payload = response.get("payload", {}) if isinstance(response, dict) else {}
    data = payload.get("data")
    return data if isinstance(data, dict) else {}


def walk(value: Any):
    yield value
    if isinstance(value, dict):
        for child in value.values():
            yield from walk(child)
    elif isinstance(value, list):
        for child in value:
            yield from walk(child)


def exact_level_from_response(response: dict[str, Any]) -> int | None:
    data = response_data(response)
    for value in walk(data):
        if not isinstance(value, list):
            continue
        for row in value:
            if isinstance(row, list) and len(row) >= 3 and row[2] == "TL":
                try:
                    return int(row[0])
                except (TypeError, ValueError):
                    return None
    for value in walk(data):
        if isinstance(value, dict) and "L" in value and "AP" in value:
            try:
                return int(value["L"])
            except (TypeError, ValueError):
                pass
    return None


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
    data = response_data(target_info)
    commanders = (data.get("gli") or {}).get("C") or []
    return {lid for lid in (row_lord_id(row) for row in commanders) if lid is not None}


def global_lids(lords_response: dict[str, Any]) -> set[int]:
    data = response_data(lords_response)
    commanders = data.get("C") or []
    return {lid for lid in (row_lord_id(row) for row in commanders) if lid is not None}


def task_target_levels(task) -> tuple[int, ...]:
    if getattr(task, "target_levels", ()):
        return tuple(int(level) for level in task.target_levels)
    if getattr(task, "target_level", None) is not None:
        return (int(task.target_level),)
    return ()


def task_has_available_commander(conn: psycopg.Connection, task) -> bool:
    normalize_available_commanders(conn)
    aid = current_aid()
    lids = tuple(int(lid) for lid in getattr(task, "commander_lids", ()) or ())
    if not lids:
        return True
    with conn.cursor() as cur:
        cur.execute(
            """
            SELECT 1
            FROM commander_state
            WHERE aid = %s
              AND lord_id = ANY(%s)
              AND available_after <= %s
              AND status NOT IN ('reserved', 'pending_cra', 'outbound')
            LIMIT 1
            """,
            (aid, list(lids), now_epoch()),
        )
        return cur.fetchone() is not None


def reserve_target_for_task(conn: psycopg.Connection, task) -> dict[str, Any] | None:
    levels = task_target_levels(task)
    if not levels:
        return None
    now = now_epoch()
    aid = current_aid()
    with conn.transaction():
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT id, kingdom_id, x_coordinate, y_coordinate, current_level, last_attacked
                FROM rbc
                WHERE aid = %s
                  AND kingdom_id = %s
                  AND current_level = ANY(%s)
                  AND COALESCE(last_attacked, 0) <= %s
                ORDER BY
                    COALESCE(last_attacked, 0),
                    floor(
                        (
                            abs(x_coordinate - %s)
                            + abs(y_coordinate - %s)
                            + random() * 95
                        ) / 35
                    ),
                    random()
                LIMIT 1
                FOR UPDATE SKIP LOCKED
                """,
                (aid, int(task.kingdom_id), list(levels), now, farm.SOURCE_X, farm.SOURCE_Y),
            )
            row = cur.fetchone()
            if row is None:
                return None
            cur.execute("UPDATE rbc SET last_attacked = %s WHERE aid = %s AND id = %s", (now + TARGET_RESERVE_SECONDS, aid, row[0]))
    return {
        "id": int(row[0]),
        "kingdom_id": int(row[1]),
        "x": int(row[2]),
        "y": int(row[3]),
        "current_level": int(row[4]),
        "target_level": int(row[4]),
        "task_name": task.name,
        "previous_last_attacked": int(row[5]) if row[5] is not None else None,
    }


def reserve_target(conn: psycopg.Connection) -> dict[str, Any] | None:
    now = now_epoch()
    aid = current_aid()
    with conn.transaction():
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT id, kingdom_id, x_coordinate, y_coordinate, current_level, last_attacked
                FROM rbc
                WHERE aid = %s
                  AND kingdom_id = %s
                  AND current_level = %s
                  AND COALESCE(last_attacked, 0) <= %s
                ORDER BY COALESCE(last_attacked, 0), random()
                LIMIT 1
                FOR UPDATE SKIP LOCKED
                """,
                (aid, SANDS_KID, TARGET_LEVEL, now),
            )
            row = cur.fetchone()
            if row is None:
                return None
            cur.execute("UPDATE rbc SET last_attacked = %s WHERE aid = %s AND id = %s", (now + TARGET_RESERVE_SECONDS, aid, row[0]))
    return {
        "id": int(row[0]),
        "kingdom_id": int(row[1]),
        "x": int(row[2]),
        "y": int(row[3]),
        "current_level": int(row[4]),
        "target_level": int(row[4]),
        "previous_last_attacked": int(row[5]) if row[5] is not None else None,
    }


def set_target_next_epoch(conn: psycopg.Connection, rbc_id: int, epoch: float, *, level: int | None = None) -> None:
    aid = current_aid()
    with conn.cursor() as cur:
        if level is None:
            cur.execute("UPDATE rbc SET last_attacked = %s WHERE aid = %s AND id = %s", (int(epoch), aid, rbc_id))
        else:
            cur.execute(
                "UPDATE rbc SET last_attacked = %s, current_level = %s WHERE aid = %s AND id = %s",
                (int(epoch), int(level), aid, rbc_id),
            )
    conn.commit()


def release_target(conn: psycopg.Connection, rbc_id: int, retry_range: tuple[float, float]) -> None:
    set_target_next_epoch(conn, rbc_id, now_epoch() + random.uniform(*retry_range))


def restore_reserved_target(conn: psycopg.Connection, target: dict[str, Any]) -> None:
    previous = target.get("previous_last_attacked")
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            "UPDATE rbc SET last_attacked = %s WHERE aid = %s AND id = %s",
            (previous, aid, int(target["id"])),
        )
    conn.commit()


def choose_commander(
    conn: psycopg.Connection,
    target_lids: set[int],
    global_available: set[int],
    allowed_lids: set[int] | tuple[int, ...] | list[int] | None = None,
) -> int | None:
    normalize_available_commanders(conn)
    aid = current_aid()
    pool = FIRST_13_COMMANDER_LIDS if allowed_lids is None else tuple(int(lid) for lid in allowed_lids)
    allowed = [lid for lid in pool if lid in target_lids and lid in global_available]
    if not allowed:
        return None
    now = now_epoch()
    with conn.transaction():
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT lord_id
                FROM commander_state
                WHERE aid = %s
                  AND lord_id = ANY(%s)
                  AND available_after <= %s
                  AND status NOT IN ('reserved', 'pending_cra', 'outbound')
                ORDER BY available_after, lord_id
                LIMIT 1
                FOR UPDATE SKIP LOCKED
                """,
                (aid, allowed, now),
            )
            row = cur.fetchone()
            if row is None:
                return None
            lid = int(row[0])
            cur.execute(
                """
                UPDATE commander_state
                SET status = 'reserved',
                    march_id = NULL,
                    target_rbc_id = NULL,
                    updated_at = %s
                WHERE lord_id = %s
                  AND aid = %s
                """,
                (now, lid, aid),
            )
    return lid


def release_commander(conn: psycopg.Connection, lid: int, *, available_after: float | None = None, status: str = "available") -> None:
    if available_after is None:
        available_after = now_epoch()
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            UPDATE commander_state
            SET available_after = %s, status = %s, march_id = NULL, target_rbc_id = NULL, updated_at = %s
            WHERE lord_id = %s
              AND aid = %s
            """,
            (int(available_after), status, now_epoch(), int(lid), aid),
        )
    conn.commit()


def normalize_available_commanders(conn: psycopg.Connection) -> None:
    now = now_epoch()
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            UPDATE commander_state
            SET status = 'available',
                march_id = NULL,
                target_rbc_id = NULL,
                updated_at = %s
            WHERE available_after <= %s
              AND aid = %s
              AND status IN ('reserved', 'pending_cra', 'outbound', 'returning', 'cra_timeout', 'cra_rejected')
            """,
            (now, now, aid),
        )
    conn.commit()


def mark_commander_pending(conn: psycopg.Connection, lid: int, rbc_id: int, available_after: float) -> None:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            UPDATE commander_state
            SET available_after = %s, status = 'pending_cra', march_id = NULL, target_rbc_id = %s, updated_at = %s
            WHERE lord_id = %s
              AND aid = %s
            """,
            (int(available_after), int(rbc_id), now_epoch(), int(lid), aid),
        )
    conn.commit()


def mark_commander_outbound(conn: psycopg.Connection, lid: int, rbc_id: int, march_id: int, available_after: float) -> None:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            UPDATE commander_state
            SET available_after = %s, status = 'outbound', march_id = %s, target_rbc_id = %s, updated_at = %s
            WHERE lord_id = %s
              AND aid = %s
            """,
            (int(available_after), int(march_id), int(rbc_id), now_epoch(), int(lid), aid),
        )
    conn.commit()


def build_attack_payload(target: dict[str, Any], lid: int, task=None) -> dict[str, Any]:
    attack_payload = task.attack_payload() if task is not None else farm.LEVEL_61.to_payload()
    return {
        "SX": farm.SOURCE_X,
        "SY": farm.SOURCE_Y,
        "TX": int(target["x"]),
        "TY": int(target["y"]),
        "KID": int(target.get("kingdom_id", SANDS_KID)),
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
        "A": attack_payload,
        "BKS": [],
        "AST": [-1, -1, -1],
        "RW": [[-1, 0] for _ in range(8)],
        "ASCT": 0,
    }


def movement_from_response(response: dict[str, Any]) -> dict[str, Any]:
    return (
        response_data(response)
        .get("AAM", {})
        .get("M", {})
    )


def active_attack_count(conn: psycopg.Connection, seconds: int) -> int:
    cutoff = now_epoch() - seconds
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute("SELECT count(*) FROM attack WHERE aid = %s AND time_created >= %s", (aid, cutoff))
        return int(cur.fetchone()[0])


def safety_check(conn: psycopg.Connection, max_cra_per_hour: int) -> None:
    if max_cra_per_hour <= 0:
        return
    count = active_attack_count(conn, 3600)
    if count >= max_cra_per_hour:
        raise SafetyStop(f"hourly_cra_cap count={count} cap={max_cra_per_hour}")


def target_next_epoch(sent_at: int, travel_seconds: float) -> int:
    landed_at = sent_at + max(0.0, travel_seconds)
    return int(landed_at + 3 * 3600 + cooldown_extra_seconds())


def record_attack(
    conn: psycopg.Connection,
    target: dict[str, Any],
    march_id: int,
    sent_at: int,
    travel_seconds: int | None,
    troop_count: int = 50,
    return_seconds: int | None = None,
    lid: int | None = None,
) -> None:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO attack (
                aid,
                kingdom_id,
                x_coordinate,
                y_coordinate,
                march_id,
                time_created,
                troop_count,
                duration,
                return_duration,
                target_kind,
                target_id,
                task_name,
                target_level,
                lord_id,
                commander_number,
                status
            )
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, 'rbc', %s, %s, %s, %s, %s, 'sent')
            ON CONFLICT (aid, march_id) DO NOTHING
            """,
            (
                aid,
                SANDS_KID,
                int(target["x"]),
                int(target["y"]),
                int(march_id),
                int(sent_at),
                int(troop_count),
                travel_seconds,
                int(return_seconds) if return_seconds is not None else None,
                int(target["id"]) if target.get("id") is not None else None,
                target.get("task_name"),
                int(target.get("target_level", target.get("current_level", TARGET_LEVEL)) or TARGET_LEVEL),
                int(lid) if lid is not None else None,
                task_scheduler.commander_human_number(lid),
            ),
        )
    conn.commit()


def record_berimond_attack(
    conn: psycopg.Connection,
    target: dict[str, Any],
    march_id: int,
    sent_at: int,
    travel_seconds: int | None,
    troop_count: int | None = None,
    return_seconds: int | None = None,
    lid: int | None = None,
    task_name: str | None = None,
) -> None:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO attack (
                aid,
                kingdom_id,
                x_coordinate,
                y_coordinate,
                march_id,
                time_created,
                troop_count,
                duration,
                return_duration,
                target_kind,
                target_id,
                task_name,
                lord_id,
                commander_number,
                status
            )
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, 'berimond_fixed', NULL, %s, %s, %s, 'sent')
            ON CONFLICT (aid, march_id) DO UPDATE SET
                kingdom_id = EXCLUDED.kingdom_id,
                x_coordinate = EXCLUDED.x_coordinate,
                y_coordinate = EXCLUDED.y_coordinate,
                time_created = EXCLUDED.time_created,
                troop_count = EXCLUDED.troop_count,
                duration = EXCLUDED.duration,
                return_duration = EXCLUDED.return_duration,
                target_kind = EXCLUDED.target_kind,
                task_name = EXCLUDED.task_name,
                lord_id = EXCLUDED.lord_id,
                commander_number = EXCLUDED.commander_number,
                status = EXCLUDED.status
            """,
            (
                aid,
                int(target.get("kingdom_id", BERIMOND_KID)),
                int(target["x"]),
                int(target["y"]),
                int(march_id),
                int(sent_at),
                int(troop_count) if troop_count is not None else None,
                int(travel_seconds) if travel_seconds is not None else None,
                int(return_seconds) if return_seconds is not None else None,
                task_name,
                int(lid) if lid is not None else None,
                task_scheduler.commander_human_number(lid),
            ),
        )
    conn.commit()


def mark_berimond_result(
    conn: psycopg.Connection,
    *,
    lid: int,
    target: dict[str, Any],
    return_seconds: int | None,
    coin_loot: int | None,
    ruby_loot: int | None,
    result_flag: int | None,
    result_at: int | None = None,
) -> bool:
    epoch = now_epoch() if result_at is None else int(result_at)
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            UPDATE attack
            SET status = 'returning',
                landed_at = %s,
                result_received_at = %s,
                return_duration = COALESCE(%s, return_duration),
                coin_loot = COALESCE(%s, coin_loot),
                ruby_loot = COALESCE(%s, ruby_loot),
                result_flag = %s
            WHERE id = (
                SELECT id
                FROM attack
                WHERE aid = %s
                  AND target_kind = 'berimond_fixed'
                  AND kingdom_id = %s
                  AND x_coordinate = %s
                  AND y_coordinate = %s
                  AND (lord_id = %s OR lord_id IS NULL)
                  AND status = 'sent'
                  AND time_created <= %s
                ORDER BY time_created DESC NULLS LAST
                LIMIT 1
            )
            """,
            (
                epoch,
                epoch,
                int(return_seconds) if return_seconds is not None else None,
                coin_loot,
                ruby_loot,
                result_flag,
                aid,
                int(target.get("kingdom_id", BERIMOND_KID)),
                int(target["x"]),
                int(target["y"]),
                int(lid),
                epoch,
            ),
        )
        updated = cur.rowcount > 0
    conn.commit()
    return updated


def mark_rbc_result(
    conn: psycopg.Connection,
    *,
    rbc_id: int,
    lid: int,
    return_seconds: int | None,
    coin_loot: int | None,
    ruby_loot: int | None,
    raw_result: dict[str, Any],
    result_at: int | None = None,
) -> bool:
    epoch = now_epoch() if result_at is None else int(result_at)
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            UPDATE attack
            SET status = 'returning',
                landed_at = %s,
                result_received_at = %s,
                return_duration = %s,
                coin_loot = COALESCE(%s, coin_loot),
                ruby_loot = COALESCE(%s, ruby_loot),
                raw_result = %s
            WHERE id = (
                SELECT id
                FROM attack
                WHERE aid = %s
                  AND target_kind = 'rbc'
                  AND target_id = %s
                  AND (lord_id = %s OR lord_id IS NULL)
                  AND status = 'sent'
                  AND time_created <= %s
                ORDER BY time_created DESC NULLS LAST
                LIMIT 1
            )
            """,
            (
                epoch,
                epoch,
                int(return_seconds) if return_seconds is not None else None,
                coin_loot,
                ruby_loot,
                Jsonb(raw_result),
                aid,
                int(rbc_id),
                int(lid),
                epoch,
            ),
        )
        updated = cur.rowcount > 0
    conn.commit()
    return updated


def attack_run_stats(conn: psycopg.Connection, *, started_at: int | None = None) -> dict[str, int]:
    aid = current_aid()
    cutoff = int(started_at or 0)
    with conn.cursor() as cur:
        cur.execute(
            """
            SELECT
                count(*)::bigint,
                count(*) FILTER (WHERE landed_at IS NOT NULL)::bigint,
                COALESCE(sum(coin_loot), 0)::bigint,
                COALESCE(sum(ruby_loot), 0)::bigint
            FROM attack
            WHERE aid = %s
              AND (%s <= 0 OR time_created >= %s)
              AND target_kind IN ('rbc', 'storm_target', 'berimond_fixed')
            """,
            (aid, cutoff, cutoff),
        )
        total, completed, coins, rubies = cur.fetchone()
    return {
        "db_attacks": int(total or 0),
        "db_completed": int(completed or 0),
        "coin_loot": int(coins or 0),
        "ruby_loot": int(rubies or 0),
    }


def send_one_attack(socket, conn: psycopg.Connection) -> bool:
    safety_check(conn, int(runtime_float(conn, "max_cra_per_hour", MAX_CRA_PER_HOUR_DEFAULT)))
    target = reserve_target(conn)
    if target is None:
        log("idle no level_61_sands_target")
        return False

    try:
        wait_for_request_slot(conn)
        target_info = socket.get_target_infos(
            BURNING_SANDS,
            farm.SOURCE_X,
            farm.SOURCE_Y,
            int(target["x"]),
            int(target["y"]),
            quiet=False,
        )
        exact_level = exact_level_from_response(target_info)
        if exact_level != TARGET_LEVEL:
            release_target(conn, target["id"], TARGET_BAD_LEVEL_RETRY_RANGE)
            if exact_level is not None:
                set_target_next_epoch(
                    conn,
                    target["id"],
                    now_epoch() + random.uniform(*TARGET_BAD_LEVEL_RETRY_RANGE),
                    level=exact_level,
                )
            log(f"skip target={target['x']}:{target['y']} exact_level={exact_level}")
            return True

        wait_for_request_slot(conn)
        lords_response = socket.get_lords()
        lid = choose_commander(conn, target_available_lids(target_info), global_lids(lords_response))
        if lid is None:
            restore_reserved_target(conn, target)
            log(f"skip target={target['x']}:{target['y']} reason=no_first_13_lid")
            return True

        wait_between_adi_and_cra()
        wait_for_request_slot(conn)
        sent_at = now_epoch()
        mark_commander_pending(conn, lid, target["id"], sent_at + HEURISTIC_RETURN_SECONDS)
        socket.send_json_command("cra", build_attack_payload(target, lid))
        response = socket.wait_for_json_response("cra")
        status = response.get("payload", {}).get("status") if isinstance(response, dict) else None
        if status not in (0, "0", None):
            release_commander(
                conn,
                lid,
                available_after=sent_at + HEURISTIC_RETURN_SECONDS + random.uniform(*COMMANDER_RETURN_HOLD_RANGE),
                status=f"cra_status_{status}",
            )
            release_target(conn, target["id"], TARGET_ERROR_RETRY_RANGE)
            log(f"cra_rejected target={target['x']}:{target['y']} lid={lid} status={status}", error=True)
            return True

        movement = movement_from_response(response)
        march_id = int(movement.get("MID"))
        travel_seconds = int(float(movement.get("TT", 0) or 0))
        provisional_return_seconds = estimated_return_seconds(travel_seconds)
        commander_available = sent_at + travel_seconds + provisional_return_seconds + random.uniform(*COMMANDER_RETURN_HOLD_RANGE)
        mark_commander_outbound(conn, lid, target["id"], march_id, commander_available)
        set_target_next_epoch(conn, target["id"], target_next_epoch(sent_at, travel_seconds), level=TARGET_LEVEL)
        record_attack(conn, target, march_id, sent_at, travel_seconds, return_seconds=None, lid=lid)
        log(
            f"cra_sent target={target['x']}:{target['y']} lid={lid} mid={march_id} "
            f"travel={travel_seconds} commander_after={int(commander_available)}"
        )
        return True
    except Exception as exc:
        release_target(conn, target["id"], TARGET_ERROR_RETRY_RANGE)
        log(f"attack_exception target={target['x']}:{target['y']} error={exc!r}", error=True)
        if "Connection is already closed" in str(exc):
            raise SessionClosed(str(exc)) from exc
        return True


def active_storm_task(task_name: str):
    for task in task_scheduler.TASKS:
        if task.name == task_name:
            if not task.enabled:
                raise RuntimeError(f"storm task disabled: {task_name}")
            return task
    raise RuntimeError(f"storm task not found: {task_name}")


def storm_target_levels(task, raw_levels: str | None) -> tuple[int, ...]:
    if raw_levels:
        levels = tuple(sorted({int(item.strip()) for item in raw_levels.split(",") if item.strip()}))
        if levels:
            return levels
    if getattr(task, "target_levels", ()):
        return tuple(int(level) for level in task.target_levels)
    if task.target_level is not None:
        return (int(task.target_level),)
    return (60, 70, 80)


def chunk_start(value: int) -> int:
    return max(0, int(value) - (int(value) % farm.MAP_CHUNK_SIZE))


def storm_scan_pass_radius(base_radius: int) -> int:
    base = max(farm.MAP_CHUNK_SIZE * 3, int(base_radius))
    roll = random.random()
    if roll < 0.18:
        return random.randint(max(40, base - 35), max(45, base - 12))
    if roll < 0.74:
        return random.randint(max(52, base - 12), base + 24)
    return random.randint(base + 25, base + 55)


def storm_scan_chunks(center_x: int, center_y: int, radius: int) -> list[tuple[int, int]]:
    offsets: list[tuple[int, int]] = []
    steps = range(-int(radius), int(radius) + 1, farm.MAP_CHUNK_SIZE)
    for dx in steps:
        for dy in steps:
            if math.hypot(dx, dy) <= int(radius) + farm.MAP_CHUNK_SIZE / 2:
                offsets.append((dx, dy))
    random.shuffle(offsets)
    if offsets and random.random() < 0.78:
        focus_radius = random.uniform(radius * 0.25, radius * 1.05)
        offsets.sort(
            key=lambda item: abs(math.hypot(item[0], item[1]) - focus_radius)
            + random.uniform(-radius * 0.45, radius * 0.45)
        )
    return [
        (chunk_start(center_x + dx - farm.MAP_CHUNK_SIZE // 2), chunk_start(center_y + dy - farm.MAP_CHUNK_SIZE // 2))
        for dx, dy in offsets
    ]


def storm_scan_wait_seconds(args: argparse.Namespace, randomizer) -> float:
    lower = max(4.0, float(args.scan_min))
    upper = max(lower, float(args.scan_max))
    wait = random.triangular(lower, upper, (lower + upper) / 2)
    if random.random() < 0.22:
        wait += random.uniform(2.5, 9.0)
    if args.use_randomizer_gaa_wait:
        wait = max(wait, float(randomizer.gaa_waiting_time()))
    return wait


def troop_count_from_payload(payload: dict[str, Any]) -> int:
    total = 0
    for wave in payload.get("A") or []:
        if not isinstance(wave, dict):
            continue
        for side in wave.values():
            if not isinstance(side, dict):
                continue
            for slot in side.get("U") or []:
                if not isinstance(slot, list) or len(slot) < 2:
                    continue
                try:
                    troop_id = int(slot[0])
                    amount = int(slot[1])
                except (TypeError, ValueError):
                    continue
                if troop_id >= 0 and amount > 0:
                    total += amount
    return total


def build_storm_attack_payload(
    target: dict[str, Any],
    lid: int,
    task,
    *,
    source_x: int,
    source_y: int,
    hbw: int,
    ptt: int,
) -> dict[str, Any]:
    return {
        "SX": int(source_x),
        "SY": int(source_y),
        "TX": int(target["x"]),
        "TY": int(target["y"]),
        "KID": int(task.kingdom_id),
        "LID": int(lid),
        "WT": 0,
        "HBW": int(hbw),
        "BPC": 0,
        "ATT": 0,
        "AV": 0,
        "LP": 0,
        "FC": 0,
        "PTT": int(ptt),
        "SD": 0,
        "ICA": 0,
        "CD": 99,
        "A": task.attack_payload(),
        "BKS": [],
        "AST": [-1, -1, -1],
        "RW": [[-1, 0] for _ in range(8)],
        "ASCT": 0,
    }


def berimond_commander_lids(commander_count: int) -> tuple[int, ...]:
    count = max(1, int(commander_count))
    return task_scheduler.commander_range(1, count)


def build_berimond_target(args: argparse.Namespace) -> dict[str, Any]:
    return {
        "id": 0,
        "kingdom_id": BERIMOND_KID,
        "x": int(args.target_x),
        "y": int(args.target_y),
        "target_level": None,
        "task_name": "berimond_fixed",
    }


def build_berimond_attack_payload(target: dict[str, Any], lid: int, args: argparse.Namespace) -> dict[str, Any]:
    return {
        "SX": int(args.source_x),
        "SY": int(args.source_y),
        "TX": int(target["x"]),
        "TY": int(target["y"]),
        "KID": BERIMOND_KID,
        "LID": int(lid),
        "WT": 0,
        "HBW": int(args.hbw),
        "BPC": 0,
        "ATT": 0,
        "AV": BERIMOND_AV,
        "LP": 0,
        "FC": 0,
        "PTT": int(args.ptt),
        "SD": 0,
        "ICA": 0,
        "CD": 99,
        "A": berimond.ATTACK.to_payload(),
        "BKS": [],
        "AST": [-1, -1, -1],
        "RW": [[-1, 0] for _ in range(8)],
        "ASCT": 0,
    }


def wait_for_proxy_ready(timeout: float = 3.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        ensure_proxy_driver_running()
        state = load_proxy_control()
        if state.get("pending") is None:
            return
        time.sleep(0.25)
    ensure_proxy_driver_running()
    state = load_proxy_control()
    if state.get("pending") is not None:
        raise RuntimeError(f"proxy_has_pending pending={state.get('pending')!r}")


def prepare_proxy_transport(args: argparse.Namespace) -> None:
    try:
        with connect(read_connection_config()) as conn:
            ensure_bot_tables(conn)
            clear_proxy_awaiting_db(conn)
    except Exception as exc:
        raise RuntimeError(f"proxy_awaiting_db_reset_failed error={exc!r}") from exc
    state = load_proxy_control()
    state.update(
        {
            "running": True,
            "transport_only": True,
            "mode": "storm",
            "username": CURRENT_ACCOUNT_NAME,
            "aid": current_aid(),
            "account_root": str(BOT_STATE_DIR),
            "control_file": str(CONTROL_FILE),
            "pending": None,
            "last_cra": None,
            "attacks_sent": 0,
            "max_attacks": int(args.max_attacks),
            "storm_task": args.storm_task,
            "target_levels": list(storm_target_levels(active_storm_task(args.storm_task), args.levels)),
            "storm_source": {"x": int(args.source_x), "y": int(args.source_y), "hbw": int(args.hbw)},
            "storm_ptt": int(args.ptt),
            "target_fresh_seconds": int(args.target_fresh_seconds),
            "started_at": now_epoch(),
            "stopped_at": None,
            "stop_reason": None,
        }
    )
    save_proxy_control(state)


def prepare_berimond_proxy_transport(args: argparse.Namespace) -> None:
    target = build_berimond_target(args)
    lids = berimond_commander_lids(args.commander_count)
    try:
        with connect(read_connection_config()) as conn:
            ensure_bot_tables(conn)
            ensure_commander_rows(conn, lids)
            clear_proxy_awaiting_db(conn)
    except Exception as exc:
        raise RuntimeError(f"proxy_awaiting_db_reset_failed error={exc!r}") from exc
    state = load_proxy_control()
    state.update(
        {
            "running": True,
            "transport_only": False,
            "mode": "berimond",
            "username": CURRENT_ACCOUNT_NAME,
            "aid": current_aid(),
            "account_root": str(BOT_STATE_DIR),
            "control_file": str(CONTROL_FILE),
            "pending": None,
            "last_cra": None,
            "attacks_sent": 0,
            "max_attacks": int(args.max_attacks),
            "berimond_target": target,
            "berimond_source": {"x": int(args.source_x), "y": int(args.source_y), "hbw": int(args.hbw)},
            "berimond_ptt": int(args.ptt),
            "berimond_commander_lids": list(lids),
            "berimond_global_attack_cooldown_seconds": float(args.global_attack_cooldown),
            "berimond_max_cra_errors": int(args.berimond_max_cra_errors),
            "berimond_cra_error_window_seconds": int(args.berimond_error_window_seconds),
            "berimond_max_commander_out_seconds": int(args.berimond_max_commander_out_seconds),
            "berimond_alert_on_error": bool(args.berimond_alert_on_error),
            "berimond_alert_sound_path": str(args.berimond_alert_sound_path),
            "berimond_attack_interval_target_seconds": float(args.attack_interval_target),
            "cra_consecutive_errors": 0,
            "cra_error_timestamps": [],
            "started_at": now_epoch(),
            "stopped_at": None,
            "stop_reason": None,
        }
    )
    save_proxy_control(state)


def queue_proxy_pending(pending: dict[str, Any]) -> float:
    ensure_proxy_driver_running()
    wait_for_proxy_ready()
    ensure_proxy_driver_running()
    queued_at = time.time()
    state = load_proxy_control()
    if state.get("running") is False and state.get("stop_reason"):
        raise SafetyStop(f"proxy_driver_stopped reason={state.get('stop_reason')}")
    state["running"] = True
    state["transport_only"] = True
    state["pending"] = pending
    save_proxy_control(state)
    try:
        with connect(read_connection_config()) as conn:
            ensure_bot_tables(conn)
            persist_proxy_pending(conn, pending)
    except Exception as exc:
        raise RuntimeError(f"proxy_pending_db_persist_failed error={exc!r}") from exc
    return queued_at


def wait_for_gaa_sent(queued_at: float, timeout: float) -> dict[str, Any]:
    deadline = time.time() + timeout
    while time.time() < deadline:
        ensure_proxy_driver_running()
        state = load_proxy_control()
        last = state.get("last_gaa_probe")
        if isinstance(last, dict) and float(last.get("sent_at", 0) or 0) >= queued_at - 1:
            return last
        if state.get("stop_reason") == "max_attacks_already_reached":
            raise SafetyStop("proxy_max_attacks_already_reached")
        time.sleep(0.25)
    raise TimeoutError("gaa_send_timeout")


def queue_storm_gaa(ax1: int, ay1: int) -> None:
    pending = {
        "kind": "gaa_probe",
        "kid": STORM_KID,
        "ax1": int(ax1),
        "ay1": int(ay1),
        "ax2": int(ax1) + farm.MAP_CHUNK_SIZE - 1,
        "ay2": int(ay1) + farm.MAP_CHUNK_SIZE - 1,
        "queued_at": time.time(),
    }
    queued_at = queue_proxy_pending(pending)
    sent = wait_for_gaa_sent(queued_at, PROXY_PENDING_TIMEOUT)
    log(f"storm_gaa_sent chunk={sent['ax1']}:{sent['ay1']}-{sent['ax2']}:{sent['ay2']}")


def last_cra_matches_target(last_cra: Any, target_id: int) -> bool:
    if not isinstance(last_cra, dict):
        return False
    target = last_cra.get("target")
    if not isinstance(target, dict):
        return False
    try:
        return int(target.get("id")) == int(target_id)
    except (TypeError, ValueError):
        return False


def storm_target_result_status(conn: psycopg.Connection, target_id: int) -> str | None:
    aid = current_aid()
    with conn.cursor() as cur:
        cur.execute(
            """
            SELECT status, sent_at, march_id
            FROM storm_target
            WHERE aid = %s
              AND id = %s
            """,
            (aid, int(target_id)),
        )
        row = cur.fetchone()
    if row is None:
        return None
    status, sent_at, march_id = row
    if sent_at is not None and march_id is not None:
        return "accepted"
    if isinstance(status, str) and status.startswith("cra_status_"):
        return "rejected"
    if isinstance(status, str) and status.startswith("adi_"):
        return "skipped"
    return None


def wait_for_storm_cra_result(conn: psycopg.Connection, target_id: int, queued_at: float, timeout: float) -> str:
    pre_cra_deadline = queued_at + PROXY_ADI_TO_CRA_TIMEOUT
    cra_deadline: float | None = None
    while True:
        result = storm_target_result_status(conn, target_id)
        if result is not None:
            return result

        state = load_proxy_control()
        last = state.get("last_cra")
        if last_cra_matches_target(last, target_id):
            try:
                sent_at = float(last.get("sent_at") or 0)
            except (TypeError, ValueError):
                sent_at = 0.0
            if sent_at > 0:
                cra_deadline = max(cra_deadline or 0.0, sent_at + timeout)

        deadline = cra_deadline if cra_deadline is not None else pre_cra_deadline
        if time.time() >= deadline:
            grace_deadline = time.time() + PROXY_CRA_TIMEOUT_GRACE
            while time.time() < grace_deadline:
                result = storm_target_result_status(conn, target_id)
                if result is not None:
                    return result
                time.sleep(0.5)
            break

        with conn.cursor() as cur:
            cur.execute("SELECT status FROM storm_target WHERE aid = %s AND id = %s", (current_aid(), int(target_id)))
            row = cur.fetchone()
        if row is not None and isinstance(row[0], str):
            if row[0].startswith("cra_status_"):
                return "rejected"
            if row[0].startswith("adi_"):
                return "skipped"
        ensure_proxy_driver_running()
        time.sleep(0.5)
    raise TimeoutError("cra_response_timeout")


def queue_storm_cra(
    conn: psycopg.Connection,
    target: dict[str, Any],
    lid: int,
    task,
    args: argparse.Namespace,
) -> bool:
    allowed_levels = storm_target_levels(task, args.levels)
    if int(target.get("target_level") or -1) not in allowed_levels:
        release_storm_target(conn, int(target["id"]))
        release_commander(conn, lid, status="available")
        log(
            f"storm_cra_blocked_bad_level target={target['x']}:{target['y']} "
            f"level={target.get('target_level')} allowed={list(allowed_levels)}",
            error=True,
        )
        return False
    payload = build_storm_attack_payload(
        target,
        lid,
        task,
        source_x=args.source_x,
        source_y=args.source_y,
        hbw=args.hbw,
        ptt=args.ptt,
    )
    pending = {
        "kind": "adi",
        "target_kind": "storm_target",
        "target": target,
        "task_name": task.name,
        "lid": int(lid),
        "due_at": time.time(),
        "army_count": troop_count_from_payload(payload),
        "attack_payload": payload,
    }
    queued_at = queue_proxy_pending(pending)
    log(
        f"storm_adi_queued target={target['x']}:{target['y']} level={target.get('target_level')} "
        f"lid={lid} army_count={pending['army_count']}"
    )
    try:
        result = wait_for_storm_cra_result(conn, int(target["id"]), queued_at, PROXY_CRA_TIMEOUT)
    except TimeoutError as exc:
        release_commander(
            conn,
            lid,
            available_after=now_epoch() + HEURISTIC_RETURN_SECONDS + random.uniform(*COMMANDER_RETURN_HOLD_RANGE),
            status="cra_timeout",
        )
        release_storm_target(conn, int(target["id"]), status="cra_timeout")
        stop_proxy_transport("storm_cra_timeout")
        log(f"storm_cra_timeout target={target['x']}:{target['y']} lid={lid}", error=True)
        raise SafetyStop(f"storm_cra_timeout target={target['x']}:{target['y']} lid={lid}") from exc
    if result == "accepted":
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT march_id, duration
                FROM attack
                WHERE aid = %s
                  AND target_kind = 'storm_target'
                  AND target_id = %s
                ORDER BY time_created DESC NULLS LAST
                LIMIT 1
                """,
                (current_aid(), int(target["id"])),
            )
            row = cur.fetchone()
        march_id, travel = row if row is not None else (None, None)
        log(f"storm_cra_ack target={target['x']}:{target['y']} lid={lid} mid={march_id} travel_duration={travel}")
    elif result == "skipped":
        log(f"storm_adi_skipped target={target['x']}:{target['y']} lid={lid}")
        return False
    else:
        release_commander(
            conn,
            lid,
            available_after=now_epoch() + HEURISTIC_RETURN_SECONDS + random.uniform(*COMMANDER_RETURN_HOLD_RANGE),
            status="cra_rejected",
        )
        log(f"storm_cra_rejected target={target['x']}:{target['y']} lid={lid}", error=True)
        return "cra_rejected"
    return True


def send_one_storm_proxy_attack(
    conn: psycopg.Connection,
    task,
    target_levels: tuple[int, ...],
    args: argparse.Namespace,
) -> bool:
    cleanup_expired_storm_targets(conn)
    target = reserve_storm_target(conn, target_levels=target_levels, fresh_seconds=int(args.target_fresh_seconds))
    if target is None:
        return False
    lid = choose_commander(conn, set(task.commander_lids), set(task.commander_lids))
    if lid is None:
        release_storm_target(conn, int(target["id"]))
        log(f"storm_skip target={target['x']}:{target['y']} reason=no_available_lid")
        return True
    delay = max(5.0, float(task_scheduler.Randomizer().attack_send_waiting_time()))
    log(f"storm_attack_wait seconds={delay:.1f} target={target['x']}:{target['y']} lid={lid}")
    queued_to_proxy = False
    try:
        interruptible_proxy_sleep(delay)
        queued_to_proxy = True
        return queue_storm_cra(conn, target, lid, task, args)
    except BaseException:
        if not queued_to_proxy:
            release_storm_target(conn, int(target["id"]))
            release_commander(conn, lid, status="available")
        raise


def run_storm_proxy(args: argparse.Namespace) -> int:
    task = active_storm_task(args.storm_task)
    levels = storm_target_levels(task, args.levels)
    scan_radius = storm_scan_pass_radius(int(args.scan_radius))
    chunks = storm_scan_chunks(int(args.source_x), int(args.source_y), scan_radius)
    if not chunks:
        raise RuntimeError("storm_scan_no_chunks")
    randomizer = task_scheduler.Randomizer()
    attacks_sent = 0
    consecutive_cra_rejects = 0
    cursor = 0
    prepare_proxy_transport(args)
    log(
        f"storm_proxy_start task={task.name} source={args.source_x}:{args.source_y} levels={list(levels)} "
        f"max_attacks={args.max_attacks} scan={args.scan_min:.1f}-{args.scan_max:.1f}s radius={scan_radius}"
    )
    stop_reason = "bot_py_complete"
    try:
        with connect(read_connection_config()) as conn:
            ensure_bot_tables(conn)
            ensure_storm_tables(conn)
            set_runtime_value(conn, "max_cra_per_hour", args.max_cra_per_hour)
            while attacks_sent < int(args.max_attacks):
                ensure_proxy_driver_running()
                safety_check(conn, int(args.max_cra_per_hour))
                acted = send_one_storm_proxy_attack(conn, task, levels, args)
                if acted == "cra_rejected":
                    consecutive_cra_rejects += 1
                    if consecutive_cra_rejects > MAX_STORM_CONSECUTIVE_CRA_REJECTS:
                        stop_reason = f"storm_consecutive_cra_rejects count={consecutive_cra_rejects}"
                        stop_proxy_transport(stop_reason)
                        raise SafetyStop(stop_reason)
                    resume_proxy_transport_after_soft_reject("cra_rejected")
                    wait = random.uniform(25.0, 55.0)
                    log(
                        f"storm_cra_reject_tolerated consecutive={consecutive_cra_rejects} "
                        f"next_wait={wait:.1f}s"
                    )
                    interruptible_proxy_sleep(wait)
                    continue
                if acted:
                    consecutive_cra_rejects = 0
                    state = load_proxy_control()
                    attacks_sent = int(state.get("attacks_sent", attacks_sent) or attacks_sent)
                    continue
                if cursor >= len(chunks):
                    cursor = 0
                    scan_radius = storm_scan_pass_radius(int(args.scan_radius))
                    chunks = storm_scan_chunks(int(args.source_x), int(args.source_y), scan_radius)
                    log(f"storm_scan_new_pass radius={scan_radius} chunks={len(chunks)}")
                ax1, ay1 = chunks[cursor]
                cursor += 1
                queue_storm_gaa(ax1, ay1)
                wait = storm_scan_wait_seconds(args, randomizer)
                log(f"storm_scan_wait seconds={wait:.1f}")
                interruptible_proxy_sleep(wait)
    except SafetyStop as exc:
        stop_reason = str(exc)
        log(f"storm_proxy_stopped reason={exc}", error=True)
    finally:
        state = load_proxy_control()
        if state.get("running") is not False:
            stop_proxy_transport(stop_reason)
    log(f"storm_proxy_done attacks_sent={attacks_sent}")
    return 0


def run_berimond_proxy(args: argparse.Namespace) -> int:
    started_at = now_epoch()
    prepare_berimond_proxy_transport(args)
    lids = berimond_commander_lids(args.commander_count)
    log(
        f"berimond_proxy_start source={args.source_x}:{args.source_y} "
        f"target={args.target_x}:{args.target_y} max_attacks={args.max_attacks} "
        f"commander_count={args.commander_count} lids={list(lids)} "
        f"global_cooldown={args.global_attack_cooldown:.1f}s"
    )
    stop_reason = "bot_py_complete"
    try:
        with connect(read_connection_config()) as conn:
            ensure_bot_tables(conn)
            while True:
                ensure_proxy_driver_running()
                state = load_proxy_control()
                attacks_sent = int(state.get("attacks_sent", 0) or 0)
                if state.get("running") is False:
                    stop_reason = str(state.get("stop_reason") or "proxy_stopped")
                    break
                if attacks_sent >= int(args.max_attacks):
                    stop_reason = f"max_attacks count={attacks_sent}"
                    stop_proxy_transport(stop_reason)
                    break
                interruptible_proxy_sleep(0.5)
    except SafetyStop as exc:
        stop_reason = str(exc)
        log(f"berimond_proxy_stopped reason={exc}", error=True)
    finally:
        state = load_proxy_control()
        if state.get("running") is not False:
            stop_proxy_transport(stop_reason)
    try:
        with connect(read_connection_config()) as conn:
            ensure_bot_tables(conn)
            stats = attack_run_stats(conn, started_at=started_at)
    except Exception as exc:
        log(f"berimond_summary_failed error={exc!r}", error=True)
        stats = {}
    log(f"berimond_proxy_done reason={stop_reason} summary={stats}")
    return 0


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Conservative database-backed Sands/Storm runner.")
    parser.add_argument("--account-name", "--username", dest="account_name", required=True)
    parser.add_argument("--mode", choices=("sands", "storm-proxy", "berimond-proxy"), default="sands")
    parser.add_argument("--account-config", type=Path, default=DEFAULT_ACCOUNT)
    parser.add_argument("--max-attacks", type=int, default=1)
    parser.add_argument("--max-cra-per-hour", type=int, default=MAX_CRA_PER_HOUR_DEFAULT)
    parser.add_argument("--loop", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--log-dir", type=Path, default=None)
    parser.add_argument("--storm-task", default="storm_custom")
    parser.add_argument("--levels", default="60,70,80")
    parser.add_argument("--source-x", type=int, default=None)
    parser.add_argument("--source-y", type=int, default=None)
    parser.add_argument("--hbw", type=int, default=None)
    parser.add_argument("--ptt", type=int, default=None)
    parser.add_argument("--scan-min", type=float, default=STORM_SCAN_INTERVAL_RANGE[0])
    parser.add_argument("--scan-max", type=float, default=STORM_SCAN_INTERVAL_RANGE[1])
    parser.add_argument("--scan-radius", type=int, default=STORM_SCAN_RADIUS)
    parser.add_argument("--target-fresh-seconds", type=int, default=STORM_TARGET_FRESH_SECONDS)
    parser.add_argument("--use-randomizer-gaa-wait", action="store_true")
    parser.add_argument("--target-x", type=int, default=BERIMOND_TARGET_X)
    parser.add_argument("--target-y", type=int, default=BERIMOND_TARGET_Y)
    parser.add_argument("--commander-count", type=int, default=BERIMOND_COMMANDER_COUNT)
    parser.add_argument("--global-attack-cooldown", type=float, default=BERIMOND_GLOBAL_ATTACK_COOLDOWN_SECONDS)
    parser.add_argument("--attack-interval-target", type=float, default=berimond.ADI_TO_CRA_TARGET_SECONDS)
    parser.add_argument("--berimond-max-cra-errors", type=int, default=BERIMOND_MAX_CRA_ERRORS)
    parser.add_argument("--berimond-error-window-seconds", type=int, default=BERIMOND_CRA_ERROR_WINDOW_SECONDS)
    parser.add_argument("--berimond-max-commander-out-seconds", type=int, default=BERIMOND_MAX_COMMANDER_OUT_SECONDS)
    parser.add_argument("--berimond-alert-sound-path", default=BERIMOND_ALERT_SOUND_PATH)
    parser.add_argument("--berimond-alert-on-error", action=argparse.BooleanOptionalAction, default=BERIMOND_ALERT_ON_ERROR)
    args = parser.parse_args(argv)
    if args.scan_max < args.scan_min:
        parser.error("--scan-max must be >= --scan-min")
    if args.mode == "berimond-proxy":
        if args.source_x is None:
            args.source_x = BERIMOND_SOURCE_X
        if args.source_y is None:
            args.source_y = BERIMOND_SOURCE_Y
        if args.hbw is None:
            args.hbw = BERIMOND_HBW
        if args.ptt is None:
            args.ptt = BERIMOND_PTT
    else:
        if args.source_x is None:
            args.source_x = STORM_SOURCE_X
        if args.source_y is None:
            args.source_y = STORM_SOURCE_Y
        if args.hbw is None:
            args.hbw = STORM_HBW
        if args.ptt is None:
            args.ptt = STORM_PTT
    if args.mode == "storm-proxy" and args.max_attacks < 1:
        parser.error("--max-attacks must be >= 1 in storm-proxy mode")
    if args.mode == "berimond-proxy" and args.max_attacks < 1:
        parser.error("--max-attacks must be >= 1 in berimond-proxy mode")
    if args.mode == "berimond-proxy" and args.commander_count < 1:
        parser.error("--commander-count must be >= 1 in berimond-proxy mode")
    if args.mode == "storm-proxy" and args.max_cra_per_hour == MAX_CRA_PER_HOUR_DEFAULT:
        args.max_cra_per_hour = args.max_attacks
    return args


def main(argv: list[str] | None = None) -> int:
    global LOG_FILE
    args = parse_args(argv)
    context = configure_account(args.account_name)
    if args.log_dir is None:
        args.log_dir = context.logs_dir
    args.log_dir.mkdir(parents=True, exist_ok=True)
    LOG_FILE = (args.log_dir / datetime.now().strftime("empire_bot_%Y%m%d_%H%M%S.log")).open("a", encoding="utf-8")
    socket = None
    attacks = 0
    consecutive_errors = 0
    try:
        if args.mode == "storm-proxy":
            return run_storm_proxy(args)
        if args.mode == "berimond-proxy":
            return run_berimond_proxy(args)
        with connect(read_connection_config()) as conn:
            ensure_bot_tables(conn)
            set_runtime_value(conn, "max_cra_per_hour", args.max_cra_per_hour)
            if args.dry_run:
                target = reserve_target(conn)
                if target is not None:
                    set_target_next_epoch(conn, target["id"], 0)
                log(f"dry_run target={target}")
                return 0
            socket = connect_sands(args.account_config)
            while True:
                if attacks >= args.max_attacks:
                    log(f"max_attacks_reached count={attacks}")
                    return 0
                try:
                    acted = send_one_attack(socket, conn)
                    consecutive_errors = 0
                    if acted:
                        attacks += 1
                    if not args.loop:
                        return 0
                    if not acted:
                        sleep_for = random.uniform(*IDLE_SLEEP_RANGE)
                        log(f"idle_sleep seconds={sleep_for:.1f}")
                        time.sleep(sleep_for)
                except SessionClosed:
                    raise
                except SafetyStop as exc:
                    log(f"safety_stop reason={exc}", error=True)
                    return 0
                except Exception as exc:
                    consecutive_errors += 1
                    log(f"loop_error consecutive={consecutive_errors} error={exc!r}", error=True)
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS:
                        return 1
                    time.sleep(random.uniform(120.0, 300.0))
    except KeyboardInterrupt:
        log("interrupted")
        return 130
    finally:
        if socket is not None and disconnect is not None:
            disconnect(socket)
        if LOG_FILE is not None:
            LOG_FILE.close()
            LOG_FILE = None


if __name__ == "__main__":
    raise SystemExit(main())
