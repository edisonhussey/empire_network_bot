
from __future__ import annotations

import argparse
import json
import os
import random
import sys
import time
from datetime import datetime
from pathlib import Path
from typing import Any

import psycopg


if __package__ in {None, ""}:
    REPO_ROOT = Path(__file__).resolve().parents[2]
    sys.path.insert(0, str(REPO_ROOT))
else:
    REPO_ROOT = Path(__file__).resolve().parents[2]

SCAN_ROOT = Path(os.environ.get("GGE_SCAN_ROOT", "/Users/edisonhussey/Desktop/scan_coordinates"))
for path in (SCAN_ROOT, SCAN_ROOT / "pygge_repo"):
    if str(path) not in sys.path:
        sys.path.insert(0, str(path))

from empire.bot.test_psql_connection import connect, read_connection_config
from empire.sand_rbc_farm import main as farm

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
DEFAULT_ACCOUNT = SCAN_ROOT / "ventrilo.ini"
DEFAULT_LOG_DIR = HERE / "logs"
SANDS_KID = 1
TARGET_LEVEL = 61
FIRST_13_COMMANDER_LIDS = (0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13)
REQUEST_INTERVAL_RANGE = (20.0, 30.0)
ADI_TO_CRA_DELAY_RANGE = (5.5, 13.0)
IDLE_SLEEP_RANGE = (55.0, 145.0)
TARGET_RESERVE_SECONDS = 12 * 60
TARGET_NO_LID_RETRY_RANGE = (18 * 60.0, 44 * 60.0)
TARGET_BAD_LEVEL_RETRY_RANGE = (2.5 * 3600.0, 4.0 * 3600.0)
TARGET_ERROR_RETRY_RANGE = (21 * 60.0, 53 * 60.0)
HEURISTIC_RETURN_SECONDS = 20 * 60
COMMANDER_RETURN_HOLD_RANGE = (45.0, 180.0)
MAX_CRA_PER_HOUR_DEFAULT = 3
MAX_CONSECUTIVE_ERRORS = 2
LOG_FILE = None


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


def ensure_bot_tables(conn: psycopg.Connection) -> None:
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
        for lid in FIRST_13_COMMANDER_LIDS:
            cur.execute(
                """
                INSERT INTO commander_state (lord_id, available_after, status, updated_at)
                VALUES (%s, 0, 'available', 0)
                ON CONFLICT (lord_id) DO NOTHING
                """,
                (lid,),
            )
    conn.commit()


def runtime_float(conn: psycopg.Connection, key: str, default: float = 0.0) -> float:
    with conn.cursor() as cur:
        cur.execute("SELECT value_text FROM bot_runtime_state WHERE key = %s", (key,))
        row = cur.fetchone()
    if row is None:
        return default
    try:
        return float(row[0])
    except (TypeError, ValueError):
        return default


def set_runtime_value(conn: psycopg.Connection, key: str, value: float | int | str) -> None:
    with conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO bot_runtime_state (key, value_text)
            VALUES (%s, %s)
            ON CONFLICT (key) DO UPDATE SET value_text = EXCLUDED.value_text
            """,
            (key, str(value)),
        )
    conn.commit()


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


def reserve_target(conn: psycopg.Connection) -> dict[str, Any] | None:
    now = now_epoch()
    with conn.transaction():
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT id, kingdom_id, x_coordinate, y_coordinate, current_level, last_attacked
                FROM rbc
                WHERE kingdom_id = %s
                  AND current_level = %s
                  AND COALESCE(last_attacked, 0) <= %s
                ORDER BY COALESCE(last_attacked, 0), random()
                LIMIT 1
                FOR UPDATE SKIP LOCKED
                """,
                (SANDS_KID, TARGET_LEVEL, now),
            )
            row = cur.fetchone()
            if row is None:
                return None
            cur.execute("UPDATE rbc SET last_attacked = %s WHERE id = %s", (now + TARGET_RESERVE_SECONDS, row[0]))
    return {
        "id": int(row[0]),
        "kingdom_id": int(row[1]),
        "x": int(row[2]),
        "y": int(row[3]),
        "current_level": int(row[4]),
        "previous_last_attacked": int(row[5]) if row[5] is not None else None,
    }


def set_target_next_epoch(conn: psycopg.Connection, rbc_id: int, epoch: float, *, level: int | None = None) -> None:
    with conn.cursor() as cur:
        if level is None:
            cur.execute("UPDATE rbc SET last_attacked = %s WHERE id = %s", (int(epoch), rbc_id))
        else:
            cur.execute(
                "UPDATE rbc SET last_attacked = %s, current_level = %s WHERE id = %s",
                (int(epoch), int(level), rbc_id),
            )
    conn.commit()


def release_target(conn: psycopg.Connection, rbc_id: int, retry_range: tuple[float, float]) -> None:
    set_target_next_epoch(conn, rbc_id, now_epoch() + random.uniform(*retry_range))


def restore_reserved_target(conn: psycopg.Connection, target: dict[str, Any]) -> None:
    previous = target.get("previous_last_attacked")
    with conn.cursor() as cur:
        cur.execute(
            "UPDATE rbc SET last_attacked = %s WHERE id = %s",
            (previous, int(target["id"])),
        )
    conn.commit()


def choose_commander(conn: psycopg.Connection, target_lids: set[int], global_available: set[int]) -> int | None:
    allowed = [lid for lid in FIRST_13_COMMANDER_LIDS if lid in target_lids and lid in global_available]
    if not allowed:
        return None
    now = now_epoch()
    with conn.transaction():
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT lord_id
                FROM commander_state
                WHERE lord_id = ANY(%s)
                  AND available_after <= %s
                ORDER BY lord_id
                LIMIT 1
                FOR UPDATE SKIP LOCKED
                """,
                (allowed, now),
            )
            row = cur.fetchone()
            if row is None:
                return None
            lid = int(row[0])
            cur.execute(
                """
                UPDATE commander_state
                SET status = 'reserved', updated_at = %s
                WHERE lord_id = %s
                """,
                (now, lid),
            )
    return lid


def release_commander(conn: psycopg.Connection, lid: int, *, available_after: float | None = None, status: str = "available") -> None:
    if available_after is None:
        available_after = now_epoch()
    with conn.cursor() as cur:
        cur.execute(
            """
            UPDATE commander_state
            SET available_after = %s, status = %s, march_id = NULL, target_rbc_id = NULL, updated_at = %s
            WHERE lord_id = %s
            """,
            (int(available_after), status, now_epoch(), int(lid)),
        )
    conn.commit()


def mark_commander_pending(conn: psycopg.Connection, lid: int, rbc_id: int, available_after: float) -> None:
    with conn.cursor() as cur:
        cur.execute(
            """
            UPDATE commander_state
            SET available_after = %s, status = 'pending_cra', march_id = NULL, target_rbc_id = %s, updated_at = %s
            WHERE lord_id = %s
            """,
            (int(available_after), int(rbc_id), now_epoch(), int(lid)),
        )
    conn.commit()


def mark_commander_outbound(conn: psycopg.Connection, lid: int, rbc_id: int, march_id: int, available_after: float) -> None:
    with conn.cursor() as cur:
        cur.execute(
            """
            UPDATE commander_state
            SET available_after = %s, status = 'outbound', march_id = %s, target_rbc_id = %s, updated_at = %s
            WHERE lord_id = %s
            """,
            (int(available_after), int(march_id), int(rbc_id), now_epoch(), int(lid)),
        )
    conn.commit()


def build_attack_payload(target: dict[str, Any], lid: int) -> dict[str, Any]:
    return {
        "SX": farm.SOURCE_X,
        "SY": farm.SOURCE_Y,
        "TX": int(target["x"]),
        "TY": int(target["y"]),
        "KID": SANDS_KID,
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
        "A": farm.LEVEL_61.to_payload(),
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
    with conn.cursor() as cur:
        cur.execute("SELECT count(*) FROM attack WHERE time_created >= %s", (cutoff,))
        return int(cur.fetchone()[0])


def safety_check(conn: psycopg.Connection, max_cra_per_hour: int) -> None:
    if max_cra_per_hour <= 0:
        return
    count = active_attack_count(conn, 3600)
    if count >= max_cra_per_hour:
        raise SafetyStop(f"hourly_cra_cap count={count} cap={max_cra_per_hour}")


def target_next_epoch(sent_at: int, travel_seconds: float) -> int:
    landed_based = sent_at + max(0.0, travel_seconds) + 3 * 3600
    heuristic_based = sent_at + HEURISTIC_RETURN_SECONDS + 3 * 3600
    return int(min(landed_based, heuristic_based) + cooldown_extra_seconds())


def record_attack(
    conn: psycopg.Connection,
    target: dict[str, Any],
    march_id: int,
    sent_at: int,
    travel_seconds: int | None,
) -> None:
    with conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO attack (
                kingdom_id,
                x_coordinate,
                y_coordinate,
                march_id,
                time_created,
                troop_count,
                duration,
                return_duration
            )
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (march_id) DO NOTHING
            """,
            (
                SANDS_KID,
                int(target["x"]),
                int(target["y"]),
                int(march_id),
                int(sent_at),
                50,
                travel_seconds,
                HEURISTIC_RETURN_SECONDS,
            ),
        )
    conn.commit()


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
        commander_available = sent_at + travel_seconds + HEURISTIC_RETURN_SECONDS + random.uniform(*COMMANDER_RETURN_HOLD_RANGE)
        mark_commander_outbound(conn, lid, target["id"], march_id, commander_available)
        set_target_next_epoch(conn, target["id"], target_next_epoch(sent_at, travel_seconds), level=TARGET_LEVEL)
        record_attack(conn, target, march_id, sent_at, travel_seconds)
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


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Conservative database-backed Sands level-61 RBC runner.")
    parser.add_argument("--account-config", type=Path, default=DEFAULT_ACCOUNT)
    parser.add_argument("--max-attacks", type=int, default=1)
    parser.add_argument("--max-cra-per-hour", type=int, default=MAX_CRA_PER_HOUR_DEFAULT)
    parser.add_argument("--loop", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--log-dir", type=Path, default=DEFAULT_LOG_DIR)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    global LOG_FILE
    args = parse_args(argv)
    args.log_dir.mkdir(parents=True, exist_ok=True)
    LOG_FILE = (args.log_dir / datetime.now().strftime("empire_bot_%Y%m%d_%H%M%S.log")).open("a", encoding="utf-8")
    socket = None
    attacks = 0
    consecutive_errors = 0
    try:
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
