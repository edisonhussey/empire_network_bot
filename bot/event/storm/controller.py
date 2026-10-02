"""Storm Islands attack controller.

Storm has deliberately different target discovery, level validation and wait
behaviour from the ordinary RBC modes.  Those policies live here; the shared
``bot.bot`` module remains the wrapper for accounts, commanders, database
safety checks and proxy message transport.
"""

from __future__ import annotations

import argparse
import math
import random
import time
from typing import Any

import psycopg

from bot import packets
from bot import db as bot_db
from bot.event.storm import config
from bot.event.storm.database import (
    STORM_KID,
    cleanup_expired_storm_targets,
    ensure_storm_tables,
    release_storm_target,
    reserve_storm_target,
)
from bot.test_psql_connection import connect, read_connection_config


MAX_CONSECUTIVE_CRA_REJECTS = config.MAX_CONSECUTIVE_CRA_REJECTS
SCAN_INTERVAL_RANGE = config.SCAN_INTERVAL_RANGE
SCAN_RADIUS = config.SCAN_RADIUS
TARGET_FRESH_SECONDS = config.TARGET_FRESH_SECONDS
SOURCE_X = config.SOURCE_X
SOURCE_Y = config.SOURCE_Y
HBW = config.HBW
PTT = config.PTT


class StormSourceUnavailable(RuntimeError):
    """The logged-in account has no safely learned Storm source castle yet."""


def _core():
    """Import the shared wrapper lazily so ``bot.bot`` can re-export us."""

    from bot import bot as core

    return core


def active_task(task_name: str):
    core = _core()
    for task in core.task_scheduler.TASKS:
        if task.name == task_name:
            if not task.enabled:
                raise RuntimeError(f"storm task disabled: {task_name}")
            return task
    definition = config.TASKS_BY_NAME.get(task_name)
    if definition is not None:
        if not definition.enabled:
            raise RuntimeError(f"storm task disabled: {task_name}")
        return core.task_scheduler.allocate_task_definitions((definition,))[0]
    raise RuntimeError(f"storm task not found: {task_name}")


def target_levels(task, raw_levels: str | list[int] | tuple[int, ...] | None) -> tuple[int, ...]:
    if raw_levels:
        items = raw_levels.split(",") if isinstance(raw_levels, str) else raw_levels
        levels = tuple(sorted({int(str(item).strip()) for item in items if str(item).strip()}))
        if levels:
            return levels
    if getattr(task, "target_levels", ()):
        return tuple(int(level) for level in task.target_levels)
    if task.target_level is not None:
        return (int(task.target_level),)
    return (60, 70, 80)


def chunk_start(value: int) -> int:
    return max(0, int(value) - (int(value) % packets.MAP_CHUNK_SIZE))


def scan_pass_radius(base_radius: int) -> int:
    base = max(packets.MAP_CHUNK_SIZE * 3, int(base_radius))
    roll = random.random()
    if roll < 0.18:
        return random.randint(max(40, base - 35), max(45, base - 12))
    if roll < 0.74:
        return random.randint(max(52, base - 12), base + 24)
    return random.randint(base + 25, base + 55)


def scan_chunks(center_x: int, center_y: int, radius: int) -> list[tuple[int, int]]:
    offsets: list[tuple[int, int]] = []
    steps = range(-int(radius), int(radius) + 1, packets.MAP_CHUNK_SIZE)
    for dx in steps:
        for dy in steps:
            if math.hypot(dx, dy) <= int(radius) + packets.MAP_CHUNK_SIZE / 2:
                offsets.append((dx, dy))
    random.shuffle(offsets)
    if offsets and random.random() < 0.78:
        focus_radius = random.uniform(radius * 0.25, radius * 1.05)
        offsets.sort(
            key=lambda item: abs(math.hypot(item[0], item[1]) - focus_radius)
            + random.uniform(-radius * 0.45, radius * 0.45)
        )
    return [
        (
            chunk_start(center_x + dx - packets.MAP_CHUNK_SIZE // 2),
            chunk_start(center_y + dy - packets.MAP_CHUNK_SIZE // 2),
        )
        for dx, dy in offsets
    ]


def scan_wait_seconds(args: argparse.Namespace, randomizer) -> float:
    lower = max(4.0, float(args.scan_min))
    upper = max(lower, float(args.scan_max))
    wait = random.triangular(lower, upper, (lower + upper) / 2)
    if random.random() < 0.22:
        wait += random.uniform(2.5, 9.0)
    if args.use_randomizer_gaa_wait:
        wait = max(wait, float(randomizer.gaa_waiting_time()))
    return wait


def build_attack_payload(
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


def resolve_source(args: argparse.Namespace) -> tuple[int, int, str]:
    """Resolve the owned Storm castle without an unsafe coordinate fallback.

    Normal operation uses the per-account castle learned by the proxy listener
    from the login ``gbd`` packet.  A paired CLI coordinate is allowed as an
    explicit operator override, primarily for protocol debugging.
    """

    if (args.source_x is None) != (args.source_y is None):
        raise StormSourceUnavailable("storm source override requires both --source-x and --source-y")
    if args.source_x is not None:
        return int(args.source_x), int(args.source_y), "cli_override"

    core = _core()
    try:
        with connect(read_connection_config()) as conn:
            core.ensure_bot_tables(conn)
            source = bot_db.read_account_sources(conn, core.current_aid()).get(str(STORM_KID))
    except Exception as exc:
        raise StormSourceUnavailable(f"storm source lookup failed: {exc!r}") from exc

    if not isinstance(source, dict) or source.get("x") is None or source.get("y") is None:
        raise StormSourceUnavailable(
            "Storm source is not known for this account. Start mitmdump, log in again "
            "so the listener sees the gbd castle list, then retry. For a deliberate "
            "manual override, pass --source-x X --source-y Y together."
        )
    return int(source["x"]), int(source["y"]), "login_gbd"


def prepare_transport(args: argparse.Namespace) -> None:
    core = _core()
    from bot.utility.recruit.config import plan_for_account

    recruit_plan = plan_for_account(core.current_aid())
    recruit_override = getattr(args, "recruit", None)
    recruit_enabled = (
        recruit_override == "true"
        if recruit_override is not None
        else bool(recruit_plan and recruit_plan.enabled_by_default)
    )
    try:
        with connect(read_connection_config()) as conn:
            core.ensure_bot_tables(conn)
            core.clear_proxy_awaiting_db(conn)
    except Exception as exc:
        raise RuntimeError(f"proxy_awaiting_db_reset_failed error={exc!r}") from exc
    state = core.load_proxy_control()
    state.update(
        {
            "running": True,
            "transport_only": True,
            "mode": "storm",
            "username": core.CURRENT_ACCOUNT_NAME,
            "aid": core.current_aid(),
            "account_root": str(core.BOT_STATE_DIR),
            "control_file": str(core.CONTROL_FILE),
            "pending": None,
            "last_cra": None,
            "recruit_enabled": recruit_enabled,
            "recruitment": None,
            # Rejections belong to this Storm run. A previous bad configuration
            # (for example an invalid troop id) must not poison the next start.
            "cra_consecutive_errors": 0,
            "cra_error_timestamps": [],
            "attacks_sent": 0,
            "max_attacks": int(args.max_attacks),
            "storm_task": args.storm_task,
            "target_levels": list(target_levels(active_task(args.storm_task), args.levels)),
            "storm_source": {
                "x": int(args.source_x),
                "y": int(args.source_y),
                "hbw": int(args.hbw),
                "origin": str(args.source_origin),
            },
            "storm_ptt": int(args.ptt),
            "target_fresh_seconds": int(args.target_fresh_seconds),
            "started_at": core.now_epoch(),
            "stopped_at": None,
            "stop_reason": None,
        }
    )
    core.save_proxy_control(state)


def queue_gaa(ax1: int, ay1: int) -> None:
    core = _core()
    pending = {
        "kind": "gaa_probe",
        "kid": STORM_KID,
        "ax1": int(ax1),
        "ay1": int(ay1),
        "ax2": int(ax1) + packets.MAP_CHUNK_SIZE - 1,
        "ay2": int(ay1) + packets.MAP_CHUNK_SIZE - 1,
        "queued_at": time.time(),
    }
    queued_at = core.queue_proxy_pending(pending)
    sent = core.wait_for_gaa_sent(queued_at, core.PROXY_PENDING_TIMEOUT)
    core.log(f"storm_gaa_sent chunk={sent['ax1']}:{sent['ay1']}-{sent['ax2']}:{sent['ay2']}")


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


def target_result_status(conn: psycopg.Connection, target_id: int) -> str | None:
    core = _core()
    with conn.cursor() as cur:
        cur.execute(
            """
            SELECT status, sent_at, march_id
            FROM storm_target
            WHERE aid = %s
              AND id = %s
            """,
            (core.current_aid(), int(target_id)),
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


def wait_for_cra_result(conn: psycopg.Connection, target_id: int, queued_at: float, timeout: float) -> str:
    core = _core()
    pre_cra_deadline = queued_at + core.PROXY_ADI_TO_CRA_TIMEOUT
    cra_deadline: float | None = None
    while True:
        result = target_result_status(conn, target_id)
        if result is not None:
            return result

        state = core.load_proxy_control()
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
            grace_deadline = time.time() + core.PROXY_CRA_TIMEOUT_GRACE
            while time.time() < grace_deadline:
                result = target_result_status(conn, target_id)
                if result is not None:
                    return result
                time.sleep(0.5)
            break

        with conn.cursor() as cur:
            cur.execute(
                "SELECT status FROM storm_target WHERE aid = %s AND id = %s",
                (core.current_aid(), int(target_id)),
            )
            row = cur.fetchone()
        if row is not None and isinstance(row[0], str):
            if row[0].startswith("cra_status_"):
                return "rejected"
            if row[0].startswith("adi_"):
                return "skipped"
        core.ensure_proxy_driver_running()
        time.sleep(0.5)
    raise TimeoutError("cra_response_timeout")


def queue_cra(
    conn: psycopg.Connection,
    target: dict[str, Any],
    lid: int,
    task,
    args: argparse.Namespace,
) -> bool | str:
    core = _core()
    allowed_levels = target_levels(task, args.levels)
    if int(target.get("target_level") or -1) not in allowed_levels:
        release_storm_target(conn, int(target["id"]))
        core.release_commander(conn, lid, status="available")
        core.log(
            f"storm_cra_blocked_bad_level target={target['x']}:{target['y']} "
            f"level={target.get('target_level')} allowed={list(allowed_levels)}",
            error=True,
        )
        return False
    payload = build_attack_payload(
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
        "army_count": core.troop_count_from_payload(payload),
        "attack_payload": payload,
    }
    queued_at = core.queue_proxy_pending(pending)
    core.log(
        f"storm_adi_queued target={target['x']}:{target['y']} level={target.get('target_level')} "
        f"lid={lid} army_count={pending['army_count']}"
    )
    try:
        result = wait_for_cra_result(conn, int(target["id"]), queued_at, core.PROXY_CRA_TIMEOUT)
    except TimeoutError as exc:
        core.release_commander(
            conn,
            lid,
            available_after=core.now_epoch()
            + core.HEURISTIC_RETURN_SECONDS
            + random.uniform(*core.COMMANDER_RETURN_HOLD_RANGE),
            status="cra_timeout",
        )
        release_storm_target(conn, int(target["id"]), status="cra_timeout")
        core.stop_proxy_transport("storm_cra_timeout")
        core.log(f"storm_cra_timeout target={target['x']}:{target['y']} lid={lid}", error=True)
        raise core.SafetyStop(f"storm_cra_timeout target={target['x']}:{target['y']} lid={lid}") from exc
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
                (core.current_aid(), int(target["id"])),
            )
            row = cur.fetchone()
        march_id, travel = row if row is not None else (None, None)
        core.log(
            f"storm_cra_ack target={target['x']}:{target['y']} lid={lid} "
            f"mid={march_id} travel_duration={travel}"
        )
    elif result == "skipped":
        core.log(f"storm_adi_skipped target={target['x']}:{target['y']} lid={lid}")
        return False
    else:
        core.release_commander(
            conn,
            lid,
            available_after=core.now_epoch()
            + core.HEURISTIC_RETURN_SECONDS
            + random.uniform(*core.COMMANDER_RETURN_HOLD_RANGE),
            status="cra_rejected",
        )
        core.log(f"storm_cra_rejected target={target['x']}:{target['y']} lid={lid}", error=True)
        return "cra_rejected"
    return True


def send_one_attack(
    conn: psycopg.Connection,
    task,
    allowed_levels: tuple[int, ...],
    args: argparse.Namespace,
) -> bool | str:
    core = _core()
    cleanup_expired_storm_targets(conn)
    target = reserve_storm_target(
        conn,
        target_levels=allowed_levels,
        fresh_seconds=int(args.target_fresh_seconds),
    )
    if target is None:
        return False
    lid = core.choose_commander(conn, set(task.commander_lids), set(task.commander_lids))
    if lid is None:
        release_storm_target(conn, int(target["id"]))
        core.log(f"storm_skip target={target['x']}:{target['y']} reason=no_available_lid")
        return True
    delay = max(5.0, float(core.task_scheduler.Randomizer().attack_send_waiting_time()))
    core.log(f"storm_attack_wait seconds={delay:.1f} target={target['x']}:{target['y']} lid={lid}")
    queued_to_proxy = False
    try:
        core.interruptible_proxy_sleep(delay)
        queued_to_proxy = True
        return queue_cra(conn, target, lid, task, args)
    except BaseException:
        if not queued_to_proxy:
            release_storm_target(conn, int(target["id"]))
            core.release_commander(conn, lid, status="available")
        raise


def run_proxy(args: argparse.Namespace) -> int:
    core = _core()
    args.source_x, args.source_y, args.source_origin = resolve_source(args)
    task = active_task(args.storm_task)
    levels = target_levels(task, args.levels)
    radius = scan_pass_radius(int(args.scan_radius))
    chunks = scan_chunks(int(args.source_x), int(args.source_y), radius)
    if not chunks:
        raise RuntimeError("storm_scan_no_chunks")
    randomizer = core.task_scheduler.Randomizer()
    attacks_sent = 0
    consecutive_cra_rejects = 0
    cursor = 0
    prepare_transport(args)
    core.log(
        f"storm_proxy_start task={task.name} source={args.source_x}:{args.source_y} "
        f"source_origin={args.source_origin} levels={list(levels)} "
        f"max_attacks={args.max_attacks} scan={args.scan_min:.1f}-{args.scan_max:.1f}s radius={radius}"
    )
    stop_reason = "bot_py_complete"
    try:
        with connect(read_connection_config()) as conn:
            core.ensure_bot_tables(conn)
            ensure_storm_tables(conn)
            core.set_runtime_value(conn, "max_cra_per_hour", args.max_cra_per_hour)
            while attacks_sent < int(args.max_attacks):
                core.ensure_proxy_driver_running()
                core.safety_check(conn, int(args.max_cra_per_hour))
                acted = send_one_attack(conn, task, levels, args)
                if acted == "cra_rejected":
                    consecutive_cra_rejects += 1
                    if consecutive_cra_rejects > MAX_CONSECUTIVE_CRA_REJECTS:
                        stop_reason = f"storm_consecutive_cra_rejects count={consecutive_cra_rejects}"
                        core.stop_proxy_transport(stop_reason)
                        raise core.SafetyStop(stop_reason)
                    core.resume_proxy_transport_after_soft_reject("cra_rejected")
                    wait = random.uniform(25.0, 55.0)
                    core.log(
                        f"storm_cra_reject_tolerated consecutive={consecutive_cra_rejects} "
                        f"next_wait={wait:.1f}s"
                    )
                    core.interruptible_proxy_sleep(wait)
                    continue
                if acted:
                    consecutive_cra_rejects = 0
                    state = core.load_proxy_control()
                    attacks_sent = int(state.get("attacks_sent", attacks_sent) or attacks_sent)
                    continue
                if cursor >= len(chunks):
                    cursor = 0
                    radius = scan_pass_radius(int(args.scan_radius))
                    chunks = scan_chunks(int(args.source_x), int(args.source_y), radius)
                    core.log(f"storm_scan_new_pass radius={radius} chunks={len(chunks)}")
                ax1, ay1 = chunks[cursor]
                cursor += 1
                queue_gaa(ax1, ay1)
                wait = scan_wait_seconds(args, randomizer)
                core.log(f"storm_scan_wait seconds={wait:.1f}")
                core.interruptible_proxy_sleep(wait)
    except core.SafetyStop as exc:
        stop_reason = str(exc)
        core.log(f"storm_proxy_stopped reason={exc}", error=True)
    finally:
        state = core.load_proxy_control()
        if state.get("running") is not False:
            core.stop_proxy_transport(stop_reason)
    core.log(f"storm_proxy_done attacks_sent={attacks_sent}")
    return 0
