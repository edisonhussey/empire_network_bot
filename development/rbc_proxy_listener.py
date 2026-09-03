from __future__ import annotations

import asyncio
import base64
import importlib
import json
import math
import pprint
import random
import sys
import time
import zlib
from datetime import datetime
from pathlib import Path

from mitmproxy import ctx, http


def find_repo_root(path: Path) -> Path:
    for parent in (path, *path.parents):
        if (parent / "bot" / "scheduler.py").is_file():
            return parent
    return path.parents[2]


REPO_ROOT = find_repo_root(Path(__file__).resolve())
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from bot import account_context
from bot import populate_database_rbc as rbc_db
from bot.populate_database_rbc import adi_target_row, gaa_level_from_value, upsert_rbc_rows
from bot import bot
from bot.storm_database import (
    STORM_KID,
    cleanup_expired_storm_targets,
    ensure_storm_tables,
    mark_storm_result,
    record_storm_attack,
    release_storm_target,
    reserve_storm_target,
    storm_targets_from_gaa,
    upsert_storm_targets,
)
from bot.test_psql_connection import connect, read_connection_config
from bot.network_sender.websockets import OUTER_WEBSOCKET
from bot.sand_rbc_farm.main import AREA_BARRON, parse_xt_packet


bot = importlib.reload(bot)
task_scheduler = importlib.reload(importlib.import_module("bot.scheduler"))


HERE = REPO_ROOT / "bot"
CAPTURE_FOLDER = HERE / "logs"
CONTROL_LOG = HERE / "rbc_proxy_listener.log"
CONTROL_FILE = HERE / "proxy_control.json"
DEFAULT_CONTROL_FILE = CONTROL_FILE
ROLL_SECONDS = 10
CONTROL_POLL_SECONDS = 1.0
NO_TARGET_RETRY_RANGE = (55.0, 145.0)
TARGET_WEBSOCKET = OUTER_WEBSOCKET
STORM_TARGET_LEVELS_DEFAULT = (60, 70, 80)
STORM_SCAN_INTERVAL_DEFAULT = (4.0, 6.0)
STORM_SCAN_RADIUS_DEFAULT = 52
STORM_TARGET_FRESH_SECONDS_DEFAULT = 120
MAX_CONSECUTIVE_CRA_ERRORS = 2
MAX_HOURLY_CRA_ERRORS = 5
CRA_STATUS_REASONS = {
    # Observed server reject: requested LID is already assigned to another active march.
    256: "lord_in_use",
}

current_file = None
last_roll = datetime.now()
active_flow = None
websocket_flows = {}
zlib_streams = {}
db_conn = None
control_task = None
next_proxy_action_epoch = 0.0
last_client_server_header = bot.farm.SAND_SERVER_HEADER

CONSOLE_LOG_PREFIXES = (
    "berimond_",
    "storm_gaa_scan_sent",
    "storm_gaa_scan_new_pass",
    "storm_adi_sent",
    "storm_adi_ok",
    "storm_adi_skip",
    "storm_cra_pending",
    "proxy_cra_sent",
    "proxy_cra_ack",
    "proxy_cra_error",
    "proxy_cat_return",
    "proxy_stopped",
    "proxy_loop_error",
    "storm_gaa_upsert_failed",
    "rbc_upsert_failed",
    "cra_ack_ignored",
)


def estimated_return_seconds(outbound_seconds) -> int:
    helper = getattr(bot, "estimated_return_seconds", None)
    if callable(helper):
        return int(helper(outbound_seconds))
    try:
        outbound = float(outbound_seconds or 0)
    except (TypeError, ValueError):
        outbound = 0.0
    if outbound > 0:
        return int(max(0.0, outbound * 1.3))
    return 30 * 60


def cra_status_reason(status) -> str:
    try:
        status_code = int(status)
    except (TypeError, ValueError):
        return "unknown"
    return CRA_STATUS_REASONS.get(status_code, "unknown")


def control_log(message: str) -> None:
    ts = datetime.now().strftime("%Y-%m-%d %H:%M:%S.%f")[:-3]
    with CONTROL_LOG.open("a", encoding="utf-8") as handle:
        handle.write(f"[{ts}] {message}\n")
        handle.flush()
    if message.startswith(CONSOLE_LOG_PREFIXES):
        try:
            ctx.log.info(message)
        except Exception:
            pass


def control_candidates() -> list[Path]:
    paths = [DEFAULT_CONTROL_FILE]
    account_root = HERE / "account_data"
    if account_root.exists():
        paths.extend(sorted(account_root.glob("*/proxy_control.json")))
    return paths


def bind_account_from_state(state: dict) -> None:
    global CAPTURE_FOLDER, CONTROL_LOG, CONTROL_FILE
    aid = state.get("aid")
    username = state.get("username") or state.get("account_name")
    if not aid:
        return
    context = account_context.for_aid(str(aid), str(username) if username else None)
    CONTROL_FILE = context.control_file
    CAPTURE_FOLDER = context.logs_dir
    CONTROL_LOG = context.listener_log
    bot.configure_account(str(username) if username else None, str(aid))
    rbc_db.set_account_aid(str(aid))


def load_control() -> dict:
    global CONTROL_FILE
    selected_path = CONTROL_FILE if CONTROL_FILE.exists() else None
    selected_state: dict | None = None
    for path in control_candidates():
        if not path.exists():
            continue
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            continue
        if not isinstance(data, dict):
            continue
        if selected_state is None:
            selected_path = path
            selected_state = data
        if data.get("running"):
            selected_path = path
            selected_state = data
            break
    if selected_state is None:
        return {"running": False, "max_attacks": 0, "attacks_sent": 0, "pending": None}
    CONTROL_FILE = selected_path or CONTROL_FILE
    bind_account_from_state(selected_state)
    return selected_state


def save_control(data: dict) -> None:
    CONTROL_FILE.parent.mkdir(parents=True, exist_ok=True)
    tmp = CONTROL_FILE.with_suffix(CONTROL_FILE.suffix + ".tmp")
    tmp.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    tmp.replace(CONTROL_FILE)


def db():
    global db_conn
    if db_conn is None or db_conn.closed:
        db_conn = connect(read_connection_config())
        bot.ensure_bot_tables(db_conn)
        ensure_storm_tables(db_conn)
    return db_conn


def hydrate_control_awaiting(state: dict, *, include_pending: bool | None = None) -> dict:
    changed = False
    if include_pending is None:
        include_pending = bool(state.get("running"))
    try:
        if include_pending and not isinstance(state.get("pending"), dict):
            pending = bot.load_proxy_pending_db(db())
            if pending is not None:
                state["pending"] = pending
                changed = True
        if not isinstance(state.get("last_cra"), dict):
            last_cra = bot.load_proxy_last_cra_db(db())
            if last_cra is not None:
                state["last_cra"] = last_cra
                changed = True
    except Exception as exc:
        control_log(f"proxy_db_hydrate_failed error={exc!r}")
    if changed:
        save_control(state)
    return state


def pretty_json(value: str) -> str | None:
    try:
        parsed = json.loads(value)
    except json.JSONDecodeError:
        return None
    return json.dumps(parsed, indent=2, sort_keys=True, ensure_ascii=False)


def pretty_xt_packet(packet: str) -> str | None:
    if not packet.startswith("%xt%"):
        return None

    fields = packet.strip().strip("%").split("%")
    if not fields or fields[0] != "xt":
        return None

    if len(fields) >= 5 and fields[1].startswith("EmpireEx_"):
        labels = ["type", "server_header", "command", "request_id", "payload"]
    elif len(fields) >= 5:
        labels = ["type", "command", "request_id", "status", "payload"]
    else:
        labels = ["type"]

    lines = ["XT PACKET", "  fields:"]
    for index, field in enumerate(fields):
        label = labels[index] if index < len(labels) else f"field_{index}"
        lines.append(f"    {label}: {field}")

    payload = fields[-1] if fields else ""
    formatted_payload = pretty_json(payload)
    if formatted_payload is not None:
        lines.extend(["", "  json_payload:", formatted_payload])
    elif payload:
        lines.extend(["", "  payload:", pprint.pformat(payload, width=100)])

    lines.extend(["", "  raw:", packet])
    return "\n".join(lines)


def format_message_for_log(message: str) -> str:
    pretty_xt = pretty_xt_packet(message)
    if pretty_xt is not None:
        return pretty_xt

    pretty = pretty_json(message)
    if pretty is not None:
        return pretty

    return pprint.pformat(message, width=120) if "\n" not in message else message


def _decode_utf8(content: bytes) -> str | None:
    try:
        return content.decode("utf-8")
    except UnicodeDecodeError:
        return None


def _inflate_bytes(content: bytes, flow_identifier: str) -> bytes | None:
    for wbits in (zlib.MAX_WBITS, -zlib.MAX_WBITS):
        try:
            return zlib.decompress(content, wbits)
        except zlib.error:
            pass

    if content.endswith(b"\x00\x00\xff\xff"):
        decompressor = zlib_streams.get(flow_identifier)
        if decompressor is None:
            decompressor = zlib.decompressobj()
            zlib_streams[flow_identifier] = decompressor
        try:
            return decompressor.decompress(content)
        except zlib.error:
            zlib_streams.pop(flow_identifier, None)

    return None


def decode_message_content(content: bytes, *, is_text: bool, flow_identifier: str) -> str:
    decoded = _decode_utf8(content)
    if decoded is not None:
        return decoded

    inflated = _inflate_bytes(content, flow_identifier)
    if inflated is not None:
        inflated_text = _decode_utf8(inflated)
        if inflated_text is not None:
            return inflated_text

    prefix = "TEXT" if is_text else "BINARY"
    return "\n".join(
        [
            f"{prefix} FRAME",
            f"  bytes: {len(content)}",
            f"  hex: {content.hex()}",
            f"  base64: {base64.b64encode(content).decode('ascii')}",
        ]
    )


def flow_id(flow: http.HTTPFlow) -> str:
    return str(id(flow))


def is_target_websocket(flow: http.HTTPFlow) -> bool:
    return TARGET_WEBSOCKET.matches_url(flow.request.url)


def remember_websocket(flow: http.HTTPFlow) -> str:
    identifier = flow_id(flow)
    websocket_flows[identifier] = flow
    return identifier


def is_open_websocket(flow: http.HTTPFlow | None) -> bool:
    return (
        flow is not None
        and flow.websocket is not None
        and flow.websocket.timestamp_end is None
    )


def inject_packet(packet: str) -> None:
    if not is_open_websocket(active_flow):
        raise RuntimeError("no active game websocket")
    ctx.master.commands.call("inject.websocket", active_flow, False, packet.encode("utf-8"))


def xt_packet(
    command: str,
    payload: dict,
    *,
    request_id: int = 1,
    server_header: str | None = None,
) -> str:
    return "%xt%{}%{}%{}%{}%".format(
        server_header or last_client_server_header or bot.farm.SAND_SERVER_HEADER,
        command,
        int(request_id),
        json.dumps(payload, separators=(",", ":")),
    )


def create_adi_packet(
    target: dict,
    *,
    source: tuple[int, int] | None = None,
    kingdom_id: int | None = None,
) -> str:
    if source is None:
        sx, sy = bot.farm.SOURCE_X, bot.farm.SOURCE_Y
    else:
        sx, sy = int(source[0]), int(source[1])
    return xt_packet(
        "adi",
        {
            "SX": sx,
            "SY": sy,
            "TX": int(target["x"]),
            "TY": int(target["y"]),
            "KID": int(kingdom_id if kingdom_id is not None else target.get("kingdom_id", bot.SANDS_KID)),
        },
    )


def create_aci_packet(
    target: dict,
    *,
    source: tuple[int, int] | None = None,
    kingdom_id: int | None = None,
) -> str:
    if source is None:
        sx, sy = bot.farm.SOURCE_X, bot.farm.SOURCE_Y
    else:
        sx, sy = int(source[0]), int(source[1])
    return xt_packet(
        "aci",
        {
            "TX": int(target["x"]),
            "TY": int(target["y"]),
            "SX": sx,
            "SY": sy,
            "KID": int(kingdom_id if kingdom_id is not None else target.get("kingdom_id", bot.SANDS_KID)),
        },
    )


def create_gaa_packet(probe: dict) -> str:
    try:
        kid = int(probe.get("kid", 4))
        ax1 = int(probe["ax1"])
        ay1 = int(probe["ay1"])
    except (KeyError, TypeError, ValueError) as exc:
        raise ValueError(f"bad gaa probe coordinates: {probe!r}") from exc

    ax2 = int(probe.get("ax2", ax1 + bot.farm.MAP_CHUNK_SIZE - 1))
    ay2 = int(probe.get("ay2", ay1 + bot.farm.MAP_CHUNK_SIZE - 1))
    return xt_packet(
        "gaa",
        {"KID": kid, "AX1": ax1, "AY1": ay1, "AX2": ax2, "AY2": ay2},
        request_id=int(probe.get("request_id", 1) or 1),
        server_header=probe.get("server_header"),
    )


def chunk_start(value: int) -> int:
    return max(0, int(value) - (int(value) % bot.farm.MAP_CHUNK_SIZE))


def storm_scan_chunks(center_x: int, center_y: int, radius: int) -> list[tuple[int, int]]:
    offsets: list[tuple[int, int]] = []
    steps = range(-int(radius), int(radius) + 1, bot.farm.MAP_CHUNK_SIZE)
    for dx in steps:
        for dy in steps:
            if math.hypot(dx, dy) <= int(radius) + bot.farm.MAP_CHUNK_SIZE / 2:
                offsets.append((dx, dy))
    random.shuffle(offsets)
    offsets.sort(key=lambda item: math.hypot(item[0], item[1]) + random.uniform(-22.0, 22.0))
    return [
        (chunk_start(center_x + dx - bot.farm.MAP_CHUNK_SIZE // 2), chunk_start(center_y + dy - bot.farm.MAP_CHUNK_SIZE // 2))
        for dx, dy in offsets
    ]


def create_cra_packet(target: dict, lid: int) -> str:
    return xt_packet("cra", bot.build_attack_payload(target, lid))


def int_or_none(value) -> int | None:
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def active_storm_task(state: dict):
    global task_scheduler
    task_scheduler = importlib.reload(task_scheduler)
    task_name = state.get("storm_task") or "storm_custom"
    for task in task_scheduler.TASKS:
        if task.name == task_name:
            if not task.enabled:
                raise RuntimeError(f"storm task disabled: {task_name}")
            return task
    raise RuntimeError(f"storm task not found: {task_name}")


def storm_target_levels(state: dict, task) -> tuple[int, ...]:
    raw_levels = state.get("target_levels")
    if isinstance(raw_levels, list):
        levels = tuple(sorted({int(value) for value in raw_levels}))
        if levels:
            return levels
    if getattr(task, "target_levels", ()):
        return tuple(int(level) for level in task.target_levels)
    if task.target_level is not None:
        return (int(task.target_level),)
    return STORM_TARGET_LEVELS_DEFAULT


def storm_source(state: dict) -> tuple[int, int, int]:
    source = state.get("storm_source")
    if not isinstance(source, dict):
        raise RuntimeError("storm_source_missing")
    sx = int(source["x"])
    sy = int(source["y"])
    hbw = int(source.get("hbw", bot.farm.HBW_VALUE))
    return sx, sy, hbw


def storm_scan_state(state: dict) -> dict:
    scan = state.get("storm_scan")
    if not isinstance(scan, dict):
        sx, sy, _ = storm_source(state)
        scan = {
            "enabled": True,
            "center_x": sx,
            "center_y": sy,
            "radius": STORM_SCAN_RADIUS_DEFAULT,
            "min_seconds": STORM_SCAN_INTERVAL_DEFAULT[0],
            "max_seconds": STORM_SCAN_INTERVAL_DEFAULT[1],
            "next_at": 0.0,
            "cursor": 0,
            "chunks": [],
        }
        state["storm_scan"] = scan
    scan.setdefault("enabled", True)
    scan.setdefault("radius", STORM_SCAN_RADIUS_DEFAULT)
    scan.setdefault("min_seconds", STORM_SCAN_INTERVAL_DEFAULT[0])
    scan.setdefault("max_seconds", STORM_SCAN_INTERVAL_DEFAULT[1])
    scan.setdefault("next_at", 0.0)
    scan.setdefault("cursor", 0)
    scan.setdefault("chunks", [])
    return scan


def schedule_next_storm_scan(scan: dict) -> float:
    min_seconds = max(1.0, float(scan.get("min_seconds", STORM_SCAN_INTERVAL_DEFAULT[0]) or STORM_SCAN_INTERVAL_DEFAULT[0]))
    max_seconds = max(min_seconds, float(scan.get("max_seconds", STORM_SCAN_INTERVAL_DEFAULT[1]) or STORM_SCAN_INTERVAL_DEFAULT[1]))
    delay = random.uniform(min_seconds, max_seconds)
    scan["next_at"] = time.time() + delay
    return delay


def next_storm_scan_chunk(scan: dict) -> tuple[int, int]:
    center_x = int(scan.get("center_x"))
    center_y = int(scan.get("center_y"))
    radius = int(scan.get("radius", STORM_SCAN_RADIUS_DEFAULT) or STORM_SCAN_RADIUS_DEFAULT)
    chunks = scan.get("chunks")
    if not isinstance(chunks, list) or not chunks:
        chunks = storm_scan_chunks(center_x, center_y, radius)
        scan["chunks"] = [[x, y] for x, y in chunks]
        scan["cursor"] = 0
    cursor = int(scan.get("cursor", 0) or 0)
    if cursor >= len(chunks):
        chunks = storm_scan_chunks(center_x, center_y, radius)
        scan["chunks"] = [[x, y] for x, y in chunks]
        scan["cursor"] = 0
        cursor = 0
        control_log("storm_gaa_scan_new_pass")
    ax1, ay1 = chunks[cursor]
    scan["cursor"] = cursor + 1
    return int(ax1), int(ay1)


def storm_scan_due(state: dict) -> bool:
    if state.get("transport_only") or state.get("mode") != "storm" or not state.get("running") or state.get("pending"):
        return False
    scan = storm_scan_state(state)
    return bool(scan.get("enabled", True)) and time.time() >= float(scan.get("next_at", 0.0) or 0.0)


def send_storm_gaa_scan(state: dict) -> bool:
    scan = storm_scan_state(state)
    ax1, ay1 = next_storm_scan_chunk(scan)
    probe = {
        "kid": STORM_KID,
        "ax1": ax1,
        "ay1": ay1,
        "ax2": ax1 + bot.farm.MAP_CHUNK_SIZE - 1,
        "ay2": ay1 + bot.farm.MAP_CHUNK_SIZE - 1,
        "server_header": last_client_server_header,
    }
    packet = create_gaa_packet(probe)
    inject_packet(packet)
    sent_at = bot.now_epoch()
    delay = schedule_next_storm_scan(scan)
    state["last_gaa_scan"] = {
        "kid": STORM_KID,
        "ax1": probe["ax1"],
        "ay1": probe["ay1"],
        "ax2": probe["ax2"],
        "ay2": probe["ay2"],
        "sent_at": sent_at,
    }
    save_control(state)
    control_log(
        f"storm_gaa_scan_sent kid={STORM_KID} chunk={probe['ax1']}:{probe['ay1']}-{probe['ax2']}:{probe['ay2']} "
        f"next_in={delay:.1f}s"
    )
    return True


def build_task_attack_payload(target: dict, lid: int, task, state: dict) -> dict:
    sx, sy, hbw = storm_source(state)
    ptt = int(state.get("storm_ptt", 1) or 1)
    return {
        "SX": sx,
        "SY": sy,
        "TX": int(target["x"]),
        "TY": int(target["y"]),
        "KID": int(task.kingdom_id),
        "LID": int(lid),
        "WT": 0,
        "HBW": hbw,
        "BPC": 0,
        "ATT": 0,
        "AV": 0,
        "LP": 0,
        "FC": 0,
        "PTT": ptt,
        "SD": 0,
        "ICA": 0,
        "CD": 99,
        "A": task.attack_payload(),
        "BKS": [],
        "AST": [-1, -1, -1],
        "RW": [[-1, 0] for _ in range(8)],
        "ASCT": 0,
    }


def troop_count_from_payload(payload: dict) -> int:
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


def target_from_pending(pending: dict | None) -> dict | None:
    if not isinstance(pending, dict):
        return None
    target = pending.get("target")
    return target if isinstance(target, dict) else None


def same_target(payload: dict, target: dict) -> bool:
    row = adi_target_row(payload)
    if row is None:
        return False
    return int(row[0]) == int(target["kingdom_id"]) and int(row[1]) == int(target["x"]) and int(row[2]) == int(target["y"])


def raw_target_available_lids(payload: dict) -> set[int]:
    commanders = (payload.get("gli") or {}).get("C") or []
    return {lid for lid in (bot.row_lord_id(row) for row in commanders) if lid is not None}


def adi_payload_matches_target(payload: dict, target: dict) -> bool:
    ai = (payload.get("gaa") or {}).get("AI")
    if not isinstance(ai, list) or len(ai) < 3:
        return True
    try:
        packet_kid = int(payload.get("KID", (payload.get("gaa") or {}).get("KID", target["kingdom_id"])))
        return (
            packet_kid == int(target["kingdom_id"])
            and int(ai[1]) == int(target["x"])
            and int(ai[2]) == int(target["y"])
        )
    except (TypeError, ValueError):
        return False


def pause_next_action(seconds_range: tuple[float, float]) -> None:
    global next_proxy_action_epoch
    next_proxy_action_epoch = time.time() + random.uniform(*seconds_range)


def stop_control(state: dict, reason: str, *, clear_awaiting: bool = True) -> None:
    state["running"] = False
    state["pending"] = None
    state["stopped_at"] = bot.now_epoch()
    state["stop_reason"] = reason
    save_control(state)
    if clear_awaiting:
        try:
            bot.clear_proxy_awaiting_db(db())
        except Exception as exc:
            control_log(f"proxy_db_clear_failed reason={reason} error={exc!r}")
    control_log(f"proxy_stopped reason={reason}")


def increment_attacks_sent(state: dict) -> None:
    attacks_sent = int(state.get("attacks_sent", 0) or 0) + 1
    state["attacks_sent"] = attacks_sent
    max_attacks = int(state.get("max_attacks", 0) or 0)
    if max_attacks and attacks_sent >= max_attacks:
        stop_control(state, f"max_attacks count={attacks_sent}", clear_awaiting=False)
    else:
        save_control(state)


def active_sands_tasks() -> list:
    global task_scheduler
    task_scheduler = importlib.reload(task_scheduler)
    return sorted(
        [
            task
            for task in task_scheduler.TASKS
            if task.enabled and int(task.kingdom_id) == int(bot.SANDS_KID)
        ],
        key=lambda task: task.priority,
    )


def sands_task_by_name(task_name: str | None):
    if not task_name:
        return None
    for task in task_scheduler.TASKS:
        if task.name == task_name:
            return task
    return None


def reserve_next_sands_target(state: dict) -> tuple[object | None, dict | None]:
    tasks = active_sands_tasks()
    if not tasks:
        return None, None
    cursor = int(state.get("sands_task_cursor", 0) or 0) % len(tasks)
    for offset in range(len(tasks)):
        index = (cursor + offset) % len(tasks)
        task = tasks[index]
        if not bot.task_has_available_commander(db(), task):
            continue
        target = bot.reserve_target_for_task(db(), task)
        if target is None:
            continue
        state["sands_task_cursor"] = (index + 1) % len(tasks)
        target["task_name"] = task.name
        return task, target
    return None, None


def queue_pending_cra(state: dict, target: dict, lid: int, task=None) -> None:
    due_at = time.time() + random.uniform(*bot.ADI_TO_CRA_DELAY_RANGE)
    payload = bot.build_attack_payload(target, lid, task)
    state["pending"] = {
        "kind": "cra",
        "target_kind": "rbc",
        "target": target,
        "task_name": task.name if task is not None else target.get("task_name"),
        "lid": int(lid),
        "due_at": due_at,
        "army_count": troop_count_from_payload(payload),
        "attack_payload": payload,
    }
    save_control(state)
    bot.persist_proxy_pending(db(), state["pending"])
    target_level = target.get("target_level", target.get("current_level", bot.TARGET_LEVEL))
    control_log(
        f"proxy_cra_pending target={target['x']}:{target['y']} kid={target['kingdom_id']} "
        f"task={state['pending'].get('task_name')} level={target_level} "
        f"lid={lid} army_count={state['pending']['army_count']} "
        f"due_in={due_at - time.time():.1f}s"
    )


def queue_pending_storm_cra(state: dict, target: dict, lid: int, task) -> None:
    randomizer = task_scheduler.Randomizer()
    due_at = time.time() + float(randomizer.attack_send_waiting_time())
    payload = build_task_attack_payload(target, lid, task, state)
    state["pending"] = {
        "kind": "cra",
        "target_kind": "storm_target",
        "target": target,
        "task_name": task.name,
        "lid": int(lid),
        "due_at": due_at,
        "army_count": troop_count_from_payload(payload),
        "attack_payload": payload,
    }
    save_control(state)
    bot.persist_proxy_pending(db(), state["pending"])
    control_log(
        f"storm_cra_pending task={task.name} target={target['x']}:{target['y']} "
        f"kid={target['kingdom_id']} level={target.get('target_level')} lid={lid} "
        f"army_count={state['pending']['army_count']} due_in={due_at - time.time():.1f}s"
    )


def queue_verified_storm_cra(state: dict, pending: dict, target: dict, lid: int, task) -> None:
    randomizer = task_scheduler.Randomizer()
    due_at = time.time() + max(5.0, float(randomizer.attack_send_waiting_time()))
    payload = build_task_attack_payload(target, lid, task, state)
    pending.update(
        {
            "kind": "cra",
            "target_kind": "storm_target",
            "target": target,
            "task_name": task.name,
            "lid": int(lid),
            "due_at": due_at,
            "army_count": troop_count_from_payload(payload),
            "attack_payload": payload,
        }
    )
    state["pending"] = pending
    save_control(state)
    bot.persist_proxy_pending(db(), pending)
    control_log(
        f"storm_adi_ok task={task.name} target={target['x']}:{target['y']} "
        f"level={target.get('target_level')} lid={lid} army_count={pending['army_count']} "
        f"cra_due_in={due_at - time.time():.1f}s"
    )


def skip_pending_storm_adi(
    state: dict,
    pending: dict,
    target: dict | None,
    lid: int | None,
    reason: str,
    *,
    status: str,
    stop: bool = False,
    target_lids: set[int] | None = None,
) -> None:
    if target is not None and int(target.get("id", 0) or 0):
        release_storm_target(db(), int(target["id"]), status=status)
    if lid is not None:
        bot.release_commander(db(), int(lid), status="available")
    state["pending"] = None
    if stop:
        stop_control(state, reason, clear_awaiting=True)
    else:
        save_control(state)
        bot.clear_proxy_pending_db(db())
    lid_text = sorted(target_lids) if target_lids is not None else []
    target_text = f"{target['x']}:{target['y']}" if target is not None else "unknown"
    control_log(f"storm_adi_skip target={target_text} lid={lid} reason={reason} target_lids={lid_text}")


def process_pending_storm_adi_error(status) -> bool:
    state = hydrate_control_awaiting(load_control())
    pending = state.get("pending")
    target = target_from_pending(pending)
    if (
        not state.get("running")
        or not isinstance(pending, dict)
        or pending.get("kind") != "adi"
        or pending.get("target_kind") != "storm_target"
    ):
        return False
    lid = int_or_none(pending.get("lid"))
    reason = f"storm_adi_status_{status}"
    skip_pending_storm_adi(state, pending, target, lid, reason, status=reason, stop=True)
    return True


def process_pending_storm_adi_payload(payload: dict) -> bool:
    state = hydrate_control_awaiting(load_control())
    pending = state.get("pending")
    target = target_from_pending(pending)
    if (
        not state.get("running")
        or not isinstance(pending, dict)
        or pending.get("kind") != "adi"
        or pending.get("target_kind") != "storm_target"
        or target is None
    ):
        return False
    sent_at = float(pending.get("sent_at", 0) or 0)
    if sent_at and time.time() - sent_at > 90.0:
        return False
    if not adi_payload_matches_target(payload, target):
        return False

    task_state = dict(state)
    task_state["storm_task"] = pending.get("task_name") or state.get("storm_task") or "storm_custom"
    task = active_storm_task(task_state)
    allowed_levels = storm_target_levels(task_state, task)
    if int(target.get("target_level") or -1) not in allowed_levels:
        skip_pending_storm_adi(
            state,
            pending,
            target,
            int_or_none(pending.get("lid")),
            "storm_adi_bad_level",
            status="adi_bad_level",
        )
        return True

    target_lids = raw_target_available_lids(payload)
    requested_lid = int_or_none(pending.get("lid"))
    lid = requested_lid if requested_lid in target_lids else None
    switched = False
    if lid is None:
        if requested_lid is not None:
            bot.release_commander(db(), requested_lid, status="available")
        lid = bot.choose_commander(db(), target_lids, set(task.commander_lids))
        switched = lid is not None

    if lid is None:
        skip_pending_storm_adi(
            state,
            pending,
            target,
            requested_lid,
            "no_available_lid_in_adi",
            status="adi_no_lid",
            target_lids=target_lids,
        )
        return True

    if switched:
        control_log(
            f"storm_adi_ok target={target['x']}:{target['y']} switched_lid={requested_lid}->{lid} "
            f"target_lids={sorted(target_lids)}"
        )
    queue_verified_storm_cra(state, pending, target, int(lid), task)
    return True


def berimond_source(state: dict) -> tuple[int, int, int]:
    source = state.get("berimond_source")
    if not isinstance(source, dict):
        raise RuntimeError("berimond_source_missing")
    return int(source["x"]), int(source["y"]), int(source.get("hbw", bot.BERIMOND_HBW))


def berimond_allowed_lids(state: dict) -> tuple[int, ...]:
    raw = state.get("berimond_commander_lids")
    if isinstance(raw, list) and raw:
        return tuple(int(value) for value in raw)
    count = int(state.get("commander_count", bot.BERIMOND_COMMANDER_COUNT) or bot.BERIMOND_COMMANDER_COUNT)
    return bot.berimond_commander_lids(count)


def berimond_target(state: dict) -> dict:
    target = state.get("berimond_target")
    if isinstance(target, dict):
        return {
            "id": int(target.get("id", 0) or 0),
            "kingdom_id": bot.BERIMOND_KID,
            "x": int(target["x"]),
            "y": int(target["y"]),
            "target_level": target.get("target_level"),
            "task_name": target.get("task_name") or "berimond_fixed",
        }
    return {
        "id": 0,
        "kingdom_id": bot.BERIMOND_KID,
        "x": bot.BERIMOND_TARGET_X,
        "y": bot.BERIMOND_TARGET_Y,
        "target_level": None,
        "task_name": "berimond_fixed",
    }


def build_berimond_attack_payload(state: dict, target: dict, lid: int) -> dict:
    sx, sy, hbw = berimond_source(state)
    return {
        "SX": sx,
        "SY": sy,
        "TX": int(target["x"]),
        "TY": int(target["y"]),
        "KID": bot.BERIMOND_KID,
        "LID": int(lid),
        "WT": 0,
        "HBW": hbw,
        "BPC": 0,
        "ATT": 0,
        "AV": bot.BERIMOND_AV,
        "LP": 0,
        "FC": 0,
        "PTT": int(state.get("berimond_ptt", bot.BERIMOND_PTT) or 0),
        "SD": 0,
        "ICA": 0,
        "CD": 99,
        "A": bot.berimond.ATTACK.to_payload(),
        "BKS": [],
        "AST": [-1, -1, -1],
        "RW": [[-1, 0] for _ in range(8)],
        "ASCT": 0,
    }


def queue_verified_berimond_cra(state: dict, pending: dict, target: dict, lid: int) -> None:
    randomizer = task_scheduler.Randomizer()
    wait = float(randomizer.berimond_adi_to_cra_waiting_time())
    due_at = max(time.time() + wait, float(pending.get("global_attack_due_at", 0.0) or 0.0))
    payload = build_berimond_attack_payload(state, target, lid)
    pending.update(
        {
            "kind": "cra",
            "target_kind": "berimond_fixed",
            "target": target,
            "task_name": "berimond_fixed",
            "lid": int(lid),
            "due_at": due_at,
            "global_attack_due_at": float(pending.get("global_attack_due_at", 0.0) or 0.0),
            "army_count": troop_count_from_payload(payload),
            "attack_payload": payload,
        }
    )
    state["pending"] = pending
    save_control(state)
    bot.persist_proxy_pending(db(), pending)
    control_log(
        f"berimond_aci_ok target={target['x']}:{target['y']} lid={lid} "
        f"army_count={pending['army_count']} cra_due_in={due_at - time.time():.2f}s"
    )


def process_pending_berimond_aci_error(status) -> bool:
    state = hydrate_control_awaiting(load_control())
    pending = state.get("pending")
    if (
        not state.get("running")
        or state.get("mode") != "berimond"
        or not isinstance(pending, dict)
        or pending.get("kind") != "aci"
        or pending.get("target_kind") != "berimond_fixed"
    ):
        return False
    lid = int_or_none(pending.get("lid"))
    if lid is not None:
        bot.release_commander(db(), lid, status=f"berimond_aci_status_{status}")
    state["pending"] = None
    bot.clear_proxy_pending_db(db())
    stop_control(state, f"berimond_aci_status_{status}", clear_awaiting=True)
    control_log(f"berimond_aci_skip status={status} lid={lid} action=stopped")
    return True


def process_pending_berimond_aci_payload(payload: dict) -> bool:
    state = hydrate_control_awaiting(load_control())
    pending = state.get("pending")
    target = target_from_pending(pending)
    if (
        not state.get("running")
        or state.get("mode") != "berimond"
        or not isinstance(pending, dict)
        or pending.get("kind") != "aci"
        or pending.get("target_kind") != "berimond_fixed"
        or target is None
    ):
        return False
    sent_at = float(pending.get("sent_at", 0) or 0)
    if sent_at and time.time() - sent_at > 45.0:
        return False
    if not adi_payload_matches_target(payload, target):
        return False

    target_lids = raw_target_available_lids(payload)
    allowed_lids = set(berimond_allowed_lids(state))
    requested_lid = int_or_none(pending.get("lid"))
    lid = requested_lid if requested_lid in target_lids and requested_lid in allowed_lids else None
    if lid is None:
        if requested_lid is not None:
            bot.release_commander(db(), requested_lid, status="available")
        lid = bot.choose_commander(db(), target_lids, allowed_lids, allowed_lids)
    if lid is None:
        state["pending"] = None
        save_control(state)
        bot.clear_proxy_pending_db(db())
        pause_next_action((4.0, 7.0))
        control_log(
            f"berimond_aci_skip target={target['x']}:{target['y']} "
            f"reason=no_available_lid allowed={sorted(allowed_lids)} target_lids={sorted(target_lids)}"
        )
        return True
    if requested_lid is not None and requested_lid != lid:
        control_log(f"berimond_aci_ok switched_lid={requested_lid}->{lid} target_lids={sorted(target_lids)}")
    queue_verified_berimond_cra(state, pending, target, int(lid))
    return True


def movement_area(movement: dict, field: str) -> tuple[int, int, int] | None:
    area = movement.get(field)
    if not isinstance(area, list) or len(area) < 3:
        return None
    try:
        return int(area[0]), int(area[1]), int(area[2])
    except (TypeError, ValueError):
        return None


def rbc_id_for_area(area: tuple[int, int, int] | None) -> int | None:
    if area is None:
        return None
    area_type, x_coordinate, y_coordinate = area
    if area_type != AREA_BARRON:
        return None
    with db().cursor() as cur:
        cur.execute(
            """
            SELECT id
            FROM rbc
            WHERE aid = %s AND kingdom_id = %s AND x_coordinate = %s AND y_coordinate = %s
            """,
            (bot.current_aid(), bot.SANDS_KID, x_coordinate, y_coordinate),
        )
        row = cur.fetchone()
    return int(row[0]) if row is not None else None


def shorten_commander_return(lid: int, march_id: int | None, rbc_id: int | None, return_epoch: float) -> None:
    with db().cursor() as cur:
        cur.execute(
            "SELECT available_after FROM commander_state WHERE aid = %s AND lord_id = %s",
            (bot.current_aid(), int(lid)),
        )
        row = cur.fetchone()
        if row is None:
            return
        current_available = int(row[0] or 0)
        actual_available = int(return_epoch)
        next_available = min(current_available, actual_available) if current_available > 0 else actual_available
        now = bot.now_epoch()
        status = "returning" if next_available > now else "available"
        cur.execute(
            """
            UPDATE commander_state
            SET available_after = %s,
                status = %s,
                march_id = COALESCE(%s, march_id),
                target_rbc_id = COALESCE(%s, target_rbc_id),
                updated_at = %s
            WHERE aid = %s
              AND lord_id = %s
            """,
            (next_available, status, march_id, rbc_id, now, bot.current_aid(), int(lid)),
        )
    db().commit()


def release_pending_target(target: dict | None, target_kind: str | None, *, status: str = "seen") -> None:
    if target is None or not int(target.get("id", 0) or 0):
        return
    if target_kind == "storm_target":
        release_storm_target(db(), int(target["id"]), status=status)
    else:
        bot.release_target(db(), int(target["id"]), bot.TARGET_ERROR_RETRY_RANGE)


def next_allowed_commander_wait(target_lids: set[int], allowed_lids=None) -> int | None:
    pool = bot.FIRST_13_COMMANDER_LIDS if allowed_lids is None else tuple(int(lid) for lid in allowed_lids)
    allowed = [lid for lid in pool if lid in target_lids]
    if not allowed:
        return None
    with db().cursor() as cur:
        cur.execute(
            """
            SELECT MIN(available_after)
            FROM commander_state
            WHERE aid = %s
              AND lord_id = ANY(%s)
            """,
            (bot.current_aid(), allowed),
        )
        row = cur.fetchone()
    if row is None or row[0] is None:
        return None
    return max(0, int(row[0]) - bot.now_epoch())


def last_cra_target(state: dict) -> dict | None:
    last_cra = state.get("last_cra")
    if not isinstance(last_cra, dict):
        return None
    return target_from_pending({"target": last_cra.get("target")})


def process_live_cra_response(decoded: str) -> bool:
    parsed = parse_xt_packet(decoded.strip())
    if not parsed or parsed.get("command") != "cra":
        return False

    state = hydrate_control_awaiting(load_control(), include_pending=False)
    last_cra = state.get("last_cra") if isinstance(state.get("last_cra"), dict) else {}
    target = last_cra_target(state)
    target_kind = last_cra.get("target_kind") or "rbc"
    task_name = last_cra.get("task_name")
    lid = last_cra.get("lid")
    status = parsed.get("status")
    sent_at_raw = last_cra.get("sent_at")
    try:
        sent_age = bot.now_epoch() - int(sent_at_raw)
    except (TypeError, ValueError):
        sent_age = 999999
    if not last_cra.get("sent_by_proxy") or sent_age > 180:
        control_log(f"cra_ack_ignored reason=no_recent_proxy_cra status={status}")
        return True

    if status not in (None, "0", 0):
        reject_status = f"cra_status_{status}"
        reject_reason = cra_status_reason(status)
        now = bot.now_epoch()
        release_pending_target(target, target_kind, status=reject_status)
        if lid is not None:
            bot.release_commander(
                db(),
                int(lid),
                available_after=now + bot.HEURISTIC_RETURN_SECONDS + random.uniform(*bot.COMMANDER_RETURN_HOLD_RANGE),
                status=reject_status,
            )
        error_window = 3600
        max_window_errors = MAX_HOURLY_CRA_ERRORS - 1
        if target_kind == "berimond_fixed":
            error_window = int(state.get("berimond_cra_error_window_seconds", bot.BERIMOND_CRA_ERROR_WINDOW_SECONDS) or bot.BERIMOND_CRA_ERROR_WINDOW_SECONDS)
            max_window_errors = int(state.get("berimond_max_cra_errors", bot.BERIMOND_MAX_CRA_ERRORS) or bot.BERIMOND_MAX_CRA_ERRORS)
        hourly_errors = [
            int(value)
            for value in state.get("cra_error_timestamps", [])
            if isinstance(value, (int, float)) and now - int(value) < error_window
        ]
        hourly_errors.append(now)
        consecutive_errors = int(state.get("cra_consecutive_errors", 0) or 0) + 1
        state["last_cra"] = None
        state["pending"] = None
        state["cra_consecutive_errors"] = consecutive_errors
        state["cra_error_timestamps"] = hourly_errors
        bot.clear_proxy_pending_db(db())
        bot.clear_proxy_last_cra_db(db())
        control_log(
            (
                f"proxy_cra_error status={status} reason={reject_reason} "
                f"target={target['x']}:{target['y']} lid={lid} task={task_name}"
            )
            if target is not None
            else f"proxy_cra_error status={status} reason={reject_reason} lid={lid} task={task_name}"
        )
        stop_for_errors = (
            len(hourly_errors) > max_window_errors
            if target_kind == "berimond_fixed"
            else consecutive_errors > MAX_CONSECUTIVE_CRA_ERRORS or len(hourly_errors) >= MAX_HOURLY_CRA_ERRORS
        )
        if stop_for_errors:
            state["running"] = False
            state["stopped_at"] = now
            state["stop_reason"] = (
                f"{reject_status} consecutive={consecutive_errors} errors={len(hourly_errors)} window={error_window}s"
            )
            save_control(state)
            control_log(f"proxy_stopped reason={state['stop_reason']}")
            return True
        state["running"] = True
        state["stopped_at"] = None
        state["stop_reason"] = None
        save_control(state)
        pause_next_action(bot.REQUEST_INTERVAL_RANGE)
        control_log(
            f"proxy_cra_error_tolerated status={status} reason={reject_reason} consecutive={consecutive_errors} "
            f"errors={len(hourly_errors)} window={error_window}s max_errors={max_window_errors}"
        )
        return True

    payload = parsed.get("payload")
    attack = payload.get("AAM") if isinstance(payload, dict) else None
    movement = attack.get("M") if isinstance(attack, dict) else None
    if not isinstance(movement, dict):
        control_log("proxy_cra_ack_unparsed reason=no_movement")
        return True

    target_area = movement_area(movement, "TA")
    march_id = movement.get("MID")
    travel_seconds = int(float(movement.get("TT", 0) or 0))
    lord_id = ((attack.get("UM") or {}).get("L") or {}).get("ID") if isinstance(attack, dict) else None
    if lid is None and lord_id is not None:
        lid = int(lord_id)
    if target is None and target_area is not None:
        _, x_coordinate, y_coordinate = target_area
        target = {
            "id": 0,
            "kingdom_id": int(movement.get("KID") or bot.SANDS_KID),
            "x": x_coordinate,
            "y": y_coordinate,
        }

    if target is None or lid is None or march_id is None:
        control_log(
            f"proxy_cra_ack_unparsed reason=missing_ids mid={march_id} lid={lid} target_known={target is not None}"
        )
        return True

    sent_at = int(last_cra.get("sent_at") or bot.now_epoch())
    return_seconds = estimated_return_seconds(travel_seconds)
    commander_available = sent_at + travel_seconds + return_seconds + random.uniform(*bot.COMMANDER_RETURN_HOLD_RANGE)
    if target_kind == "storm_target" and int(target.get("id", 0) or 0):
        bot.mark_commander_outbound(db(), int(lid), int(target["id"]), int(march_id), commander_available)
        record_storm_attack(
            db(),
            target,
            march_id=int(march_id),
            sent_at=sent_at,
            travel_seconds=travel_seconds,
            return_seconds=return_seconds,
            troop_count=int_or_none(last_cra.get("army_count")),
            task_name=task_name,
            lid=int(lid),
        )
    elif target_kind == "berimond_fixed":
        bot.mark_commander_outbound(db(), int(lid), int(target.get("id", 0) or 0), int(march_id), commander_available)
        bot.record_berimond_attack(
            db(),
            target,
            int(march_id),
            sent_at,
            travel_seconds,
            troop_count=int_or_none(last_cra.get("army_count")),
            return_seconds=return_seconds,
            lid=int(lid),
            task_name=task_name or "berimond_fixed",
        )
    elif int(target.get("id", 0) or 0):
        target_level = int(target.get("target_level", target.get("current_level", bot.TARGET_LEVEL)) or bot.TARGET_LEVEL)
        troop_count = int_or_none(last_cra.get("army_count")) or 50
        bot.mark_commander_outbound(db(), int(lid), int(target["id"]), int(march_id), commander_available)
        bot.set_target_next_epoch(db(), int(target["id"]), bot.target_next_epoch(sent_at, travel_seconds), level=target_level)
        bot.record_attack(
            db(),
            target,
            int(march_id),
            sent_at,
            travel_seconds,
            troop_count=troop_count,
            return_seconds=None,
            lid=int(lid),
        )
    state["last_cra"] = None
    state["cra_consecutive_errors"] = 0
    state["cra_error_timestamps"] = [
        int(value)
        for value in state.get("cra_error_timestamps", [])
        if isinstance(value, (int, float)) and bot.now_epoch() - int(value) < 3600
    ]
    save_control(state)
    bot.clear_proxy_last_cra_db(db())
    control_log(
        f"proxy_cra_ack target={target['x']}:{target['y']} kid={target['kingdom_id']} "
        f"kind={target_kind} task={task_name} level={target.get('target_level', bot.TARGET_LEVEL)} lid={lid} mid={march_id} "
        f"army_count={last_cra.get('army_count', 'unknown')} travel_duration={travel_seconds} "
        "return_duration=pending_cat"
    )
    return True


def loot_from_result(resources) -> tuple[int | None, int | None]:
    coin_loot = None
    ruby_loot = None
    if not isinstance(resources, list):
        return coin_loot, ruby_loot
    for item in resources:
        if not isinstance(item, list) or len(item) < 2:
            continue
        key = item[0]
        amount = int_or_none(item[1])
        if key == "C1":
            coin_loot = amount
        elif key == "C2":
            ruby_loot = amount
    return coin_loot, ruby_loot


def process_live_cat_response(decoded: str) -> bool:
    parsed = parse_xt_packet(decoded.strip())
    if not parsed or parsed.get("command") != "cat" or parsed.get("status") not in (None, "0", 0):
        return False

    payload = parsed.get("payload")
    attack = payload.get("A") if isinstance(payload, dict) else None
    movement = attack.get("M") if isinstance(attack, dict) else None
    if not isinstance(movement, dict):
        return False

    lord_id = ((attack.get("UM") or {}).get("L") or {}).get("ID") if isinstance(attack, dict) else None
    try:
        lid = int(lord_id)
    except (TypeError, ValueError):
        return False
    if lid not in bot.known_commander_lids():
        return False

    source_area = movement_area(movement, "SA")
    rbc_id = rbc_id_for_area(source_area)
    packet_kid = int_or_none(movement.get("KID"))
    march_id = movement.get("MID")
    march_id_int = int(march_id) if march_id is not None else None
    return_seconds = int(float(movement.get("TT", 0) or 0))
    result_flag = int_or_none(attack.get("S")) if isinstance(attack, dict) else None
    coin_loot, ruby_loot = loot_from_result(attack.get("G") if isinstance(attack, dict) else None)
    if march_id_int is not None and packet_kid != bot.BERIMOND_KID:
        mark_storm_result(
            db(),
            march_id=march_id_int,
            result_flag=result_flag,
            return_seconds=return_seconds,
            coin_loot=coin_loot,
            ruby_loot=ruby_loot,
            raw_result=payload if isinstance(payload, dict) else {},
        )
    rbc_result_updated = False
    berimond_result_updated = False
    # CAT is a separate return movement, so its MID can differ from the original CRA MID.
    # Match RBC results by the attacked RBC plus the commander LID instead.
    if rbc_id is not None:
        rbc_result_updated = bot.mark_rbc_result(
            db(),
            rbc_id=rbc_id,
            lid=lid,
            return_seconds=return_seconds,
            coin_loot=coin_loot,
            ruby_loot=ruby_loot,
            raw_result=payload if isinstance(payload, dict) else {},
        )
    elif packet_kid == bot.BERIMOND_KID and source_area is not None:
        _, x_coordinate, y_coordinate = source_area
        berimond_result_updated = bot.mark_berimond_result(
            db(),
            lid=lid,
            target={
                "kingdom_id": bot.BERIMOND_KID,
                "x": x_coordinate,
                "y": y_coordinate,
            },
            return_seconds=return_seconds,
            coin_loot=coin_loot,
            ruby_loot=ruby_loot,
            result_flag=result_flag,
        )
    return_epoch = bot.now_epoch() + return_seconds + random.uniform(*bot.COMMANDER_RETURN_HOLD_RANGE)
    shorten_commander_return(lid, march_id_int, rbc_id, return_epoch)
    target_text = f"{source_area[1]}:{source_area[2]}" if source_area is not None else "unknown"
    control_log(
        f"proxy_cat_return target={target_text} kid={packet_kid} lid={lid} mid={march_id_int} "
        f"result_flag={result_flag} coin_loot={coin_loot} ruby_loot={ruby_loot} "
        f"return_duration={return_seconds} rbc_attack_updated={rbc_result_updated} "
        f"berimond_attack_updated={berimond_result_updated} available_after={int(return_epoch)}"
    )
    return True


def handle_storm_mode(state: dict) -> bool:
    task = active_storm_task(state)
    fresh_seconds = int(state.get("target_fresh_seconds", STORM_TARGET_FRESH_SECONDS_DEFAULT) or STORM_TARGET_FRESH_SECONDS_DEFAULT)
    target = reserve_storm_target(db(), target_levels=storm_target_levels(state, task), fresh_seconds=fresh_seconds)
    if target is None:
        control_log(
            f"storm_idle reason=no_live_target levels={list(storm_target_levels(state, task))}"
        )
        return False
    lid = bot.choose_commander(db(), set(task.commander_lids), set(task.commander_lids))
    if lid is None:
        release_storm_target(db(), int(target["id"]))
        next_wait = next_allowed_commander_wait(set(task.commander_lids))
        if next_wait is None:
            pause_next_action(bot.TARGET_NO_LID_RETRY_RANGE)
        else:
            lower = max(30.0, min(float(next_wait) + 15.0, 180.0))
            upper = max(lower + 10.0, min(float(next_wait) + 45.0, 240.0))
            pause_next_action((lower, upper))
        control_log(
            f"storm_idle reason=no_available_lid task={task.name} lids={list(task.commander_lids)} next_wait={next_wait}"
        )
        return False
    queue_pending_storm_cra(state, target, lid, task)
    return True


def next_berimond_global_due(state: dict) -> float:
    cooldown = float(state.get("berimond_global_attack_cooldown_seconds", bot.BERIMOND_GLOBAL_ATTACK_COOLDOWN_SECONDS) or 0.0)
    last_sent = float(state.get("last_global_attack_sent_at", 0.0) or 0.0)
    if cooldown <= 0.0 or last_sent <= 0.0:
        return time.time()
    randomizer = task_scheduler.Randomizer()
    return last_sent + cooldown + float(randomizer.berimond_attack_cooldown_jitter())


def handle_berimond_mode(state: dict) -> bool:
    target = berimond_target(state)
    allowed_lids = set(berimond_allowed_lids(state))
    lid = bot.choose_commander(db(), allowed_lids, allowed_lids, allowed_lids)
    if lid is None:
        next_wait = next_allowed_commander_wait(allowed_lids, allowed_lids)
        if next_wait is None:
            pause_next_action((4.0, 8.0))
        else:
            lower = max(1.0, min(float(next_wait) - 3.0, 60.0))
            upper = max(lower + 1.0, min(float(next_wait), 90.0))
            pause_next_action((lower, upper))
        control_log(f"berimond_idle reason=no_available_lid lids={sorted(allowed_lids)} next_wait={next_wait}")
        return False

    global_due = next_berimond_global_due(state)
    lead = float(state.get("berimond_attack_interval_target_seconds", 3.0) or 3.0)
    aci_due = max(time.time(), global_due - max(0.5, lead))
    pending = {
        "kind": "aci",
        "target_kind": "berimond_fixed",
        "target": target,
        "task_name": "berimond_fixed",
        "lid": int(lid),
        "due_at": aci_due,
        "sent_at": None,
        "global_attack_due_at": global_due,
    }
    state["pending"] = pending
    save_control(state)
    bot.persist_proxy_pending(db(), pending)
    control_log(
        f"berimond_aci_queued target={target['x']}:{target['y']} kid={target['kingdom_id']} "
        f"lid={lid} aci_due_in={aci_due - time.time():.2f}s global_cra_due_in={global_due - time.time():.2f}s"
    )
    return True


async def proxy_control_loop() -> None:
    global next_proxy_action_epoch

    while True:
        try:
            state = hydrate_control_awaiting(load_control())
            pending = state.get("pending")
            probe_pending = isinstance(pending, dict) and pending.get("kind") == "gaa_probe"
            scan_due = storm_scan_due(state)
            if not state.get("running") and not probe_pending:
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue
            if int(state.get("max_attacks", 0) or 0) and int(state.get("attacks_sent", 0) or 0) >= int(state.get("max_attacks", 0) or 0):
                stop_control(state, "max_attacks_already_reached")
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue
            if not probe_pending and not scan_due and time.time() < next_proxy_action_epoch:
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue
            if not is_open_websocket(active_flow):
                if probe_pending:
                    control_log("gaa_probe_wait reason=no_active_websocket")
                else:
                    control_log("proxy_wait reason=no_active_websocket")
                    pause_next_action((15.0, 35.0))
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue

            if scan_due:
                send_storm_gaa_scan(state)
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue

            if probe_pending:
                packet = create_gaa_packet(pending)
                inject_packet(packet)
                sent_at = bot.now_epoch()
                ax1 = int(pending["ax1"])
                ay1 = int(pending["ay1"])
                ax2 = int(pending.get("ax2", ax1 + bot.farm.MAP_CHUNK_SIZE - 1))
                ay2 = int(pending.get("ay2", ay1 + bot.farm.MAP_CHUNK_SIZE - 1))
                kid = int(pending.get("kid", 4))
                state["pending"] = None
                state["last_gaa_probe"] = {
                    "kid": kid,
                    "ax1": ax1,
                    "ay1": ay1,
                    "ax2": ax2,
                    "ay2": ay2,
                    "sent_at": sent_at,
                    "packet": packet,
                }
                save_control(state)
                bot.clear_proxy_pending_db(db())
                control_log(f"gaa_probe_sent kid={kid} chunk={ax1}:{ay1}-{ax2}:{ay2}")
                pause_next_action(bot.REQUEST_INTERVAL_RANGE)
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue

            if isinstance(pending, dict) and pending.get("kind") == "adi" and pending.get("target_kind") == "storm_target":
                due_at = float(pending.get("due_at", 0.0) or 0.0)
                if time.time() < due_at:
                    await asyncio.sleep(CONTROL_POLL_SECONDS)
                    continue
                target = target_from_pending(pending)
                lid = int_or_none(pending.get("lid"))
                if target is None or lid is None:
                    state["pending"] = None
                    save_control(state)
                    bot.clear_proxy_pending_db(db())
                    continue
                sx, sy, _ = storm_source(state)
                inject_packet(create_adi_packet(target, source=(sx, sy), kingdom_id=STORM_KID))
                pending["sent_at"] = time.time()
                state["pending"] = pending
                save_control(state)
                bot.persist_proxy_pending(db(), pending)
                control_log(
                    f"storm_adi_sent target={target['x']}:{target['y']} kid={target['kingdom_id']} "
                    f"level={target.get('target_level')} lid={lid}"
                )
                pause_next_action(bot.REQUEST_INTERVAL_RANGE)
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue

            if isinstance(pending, dict) and pending.get("kind") == "aci" and pending.get("target_kind") == "berimond_fixed":
                due_at = float(pending.get("due_at", 0.0) or 0.0)
                if time.time() < due_at:
                    await asyncio.sleep(CONTROL_POLL_SECONDS)
                    continue
                sent_at = float(pending.get("sent_at", 0.0) or 0.0)
                if sent_at:
                    if time.time() - sent_at > 45.0:
                        lid = int_or_none(pending.get("lid"))
                        if lid is not None:
                            bot.release_commander(db(), lid, status="berimond_aci_timeout")
                        state["pending"] = None
                        save_control(state)
                        bot.clear_proxy_pending_db(db())
                        control_log(f"berimond_aci_skip reason=timeout lid={lid}")
                        pause_next_action((4.0, 8.0))
                    await asyncio.sleep(CONTROL_POLL_SECONDS)
                    continue
                target = target_from_pending(pending)
                lid = int_or_none(pending.get("lid"))
                if target is None or lid is None:
                    state["pending"] = None
                    save_control(state)
                    bot.clear_proxy_pending_db(db())
                    continue
                sx, sy, _ = berimond_source(state)
                inject_packet(create_aci_packet(target, source=(sx, sy), kingdom_id=bot.BERIMOND_KID))
                pending["sent_at"] = time.time()
                state["pending"] = pending
                save_control(state)
                bot.persist_proxy_pending(db(), pending)
                control_log(
                    f"berimond_aci_sent target={target['x']}:{target['y']} kid={target['kingdom_id']} "
                    f"lid={lid} global_cra_due_in={float(pending.get('global_attack_due_at', 0.0) or 0.0) - time.time():.2f}s"
                )
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue

            if isinstance(pending, dict) and pending.get("kind") == "cra":
                due_at = float(pending.get("due_at", 0.0) or 0.0)
                if state.get("mode") == "berimond":
                    global_due = float(pending.get("global_attack_due_at", 0.0) or 0.0)
                    if global_due <= 0.0:
                        global_due = next_berimond_global_due(state)
                        pending["global_attack_due_at"] = global_due
                    due_at = max(due_at, global_due)
                    if due_at != float(pending.get("due_at", 0.0) or 0.0):
                        pending["due_at"] = due_at
                        state["pending"] = pending
                        save_control(state)
                        bot.persist_proxy_pending(db(), pending)
                if time.time() < due_at:
                    await asyncio.sleep(CONTROL_POLL_SECONDS)
                    continue
                target = target_from_pending(pending)
                lid = pending.get("lid")
                if target is None or lid is None:
                    state["pending"] = None
                    save_control(state)
                    bot.clear_proxy_pending_db(db())
                    continue
                attack_payload = pending.get("attack_payload")
                if not isinstance(attack_payload, dict):
                    if pending.get("target_kind") == "berimond_fixed":
                        attack_payload = build_berimond_attack_payload(state, target, int(lid))
                    else:
                        attack_payload = bot.build_attack_payload(target, int(lid))
                packet = xt_packet("cra", attack_payload)
                army_count = troop_count_from_payload(attack_payload)
                sent_at = bot.now_epoch()
                commander_next = sent_at + bot.HEURISTIC_RETURN_SECONDS + random.uniform(*bot.COMMANDER_RETURN_HOLD_RANGE)
                bot.mark_commander_pending(db(), int(lid), int(target["id"]), commander_next)
                inject_packet(packet)
                if state.get("mode") == "berimond":
                    state["last_global_attack_sent_at"] = float(sent_at)
                state["pending"] = None
                state["last_cra"] = {
                    "target": target,
                    "target_kind": pending.get("target_kind", "rbc"),
                    "task_name": pending.get("task_name"),
                    "lid": int(lid),
                    "sent_at": sent_at,
                    "sent_by_proxy": True,
                    "army_count": army_count,
                }
                bot.clear_proxy_pending_db(db())
                bot.persist_proxy_last_cra(db(), state["last_cra"])
                control_log(
                    f"proxy_cra_sent target={target['x']}:{target['y']} kid={target['kingdom_id']} "
                    f"kind={pending.get('target_kind', 'rbc')} task={pending.get('task_name')} "
                    f"level={target.get('target_level', bot.TARGET_LEVEL)} lid={lid} mid=pending "
                    f"army_count={army_count} travel_duration=pending"
                )
                increment_attacks_sent(state)
                if state.get("mode") == "berimond":
                    next_proxy_action_epoch = 0.0
                else:
                    pause_next_action(bot.REQUEST_INTERVAL_RANGE)
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue

            if pending:
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue

            if state.get("transport_only"):
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue

            if state.get("mode") == "storm":
                handle_storm_mode(state)
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue

            if state.get("mode") == "berimond":
                handle_berimond_mode(state)
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue

            task, target = reserve_next_sands_target(state)
            if target is None:
                control_log("proxy_idle reason=no_sands_task_target")
                pause_next_action(NO_TARGET_RETRY_RANGE)
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue
            inject_packet(create_adi_packet(target))
            state["pending"] = {
                "kind": "adi",
                "target": target,
                "task_name": task.name if task is not None else target.get("task_name"),
                "sent_at": bot.now_epoch(),
            }
            save_control(state)
            control_log(
                f"proxy_adi_sent target={target['x']}:{target['y']} kid={target['kingdom_id']} "
                f"task={state['pending'].get('task_name')} expected_level={target.get('target_level')}"
            )
            pause_next_action(bot.REQUEST_INTERVAL_RANGE)
        except Exception as exc:
            control_log(f"proxy_loop_error error={exc!r}")
            pause_next_action((120.0, 300.0))
        await asyncio.sleep(CONTROL_POLL_SECONDS)


def open_capture_file(ts: datetime):
    global current_file, last_roll

    if current_file is None or (ts - last_roll).total_seconds() >= ROLL_SECONDS:
        if current_file is not None:
            current_file.close()
        CAPTURE_FOLDER.mkdir(parents=True, exist_ok=True)
        path = CAPTURE_FOLDER / f"gge_{ts.strftime('%Y%m%d_%H%M%S')}.log"
        current_file = path.open("a", encoding="utf-8")
        last_roll = ts
        control_log(f"capture_file path={path}")

    return current_file


def parse_live_payload(decoded: str, command: str) -> dict | None:
    parsed = parse_xt_packet(decoded.strip())
    if not parsed or parsed.get("command") != command or parsed.get("status") not in {None, "0", 0}:
        return None
    payload = parsed.get("payload")
    return payload if isinstance(payload, dict) else None


def process_storm_gaa_payload(payload: dict) -> bool:
    try:
        packet_kid = int(payload.get("KID"))
    except (TypeError, ValueError):
        return False
    if packet_kid != STORM_KID:
        return False

    try:
        targets = storm_targets_from_gaa(payload)
        expired = cleanup_expired_storm_targets(db())
        changed = upsert_storm_targets(db(), targets)
        control_log(
            f"storm_gaa_upsert kid={packet_kid} candidates={len(targets)} "
            f"db_writes={changed} expired_deleted={expired}"
        )
    except Exception as exc:
        if db_conn is not None:
            db_conn.rollback()
        control_log(f"storm_gaa_upsert_failed error={exc!r}")
    return True


def remember_client_header(decoded: str) -> None:
    global last_client_server_header

    parsed = parse_xt_packet(decoded.strip())
    if not parsed:
        return
    header = parsed.get("server_header")
    if isinstance(header, str) and header.startswith("EmpireEx_"):
        last_client_server_header = header


def live_rbc_rows_from_payload(payload: dict) -> tuple[int | None, list[tuple[int, int, int, int]]]:
    rows: list[tuple[int, int, int, int]] = []
    try:
        packet_kid = int(payload["KID"])
    except (KeyError, TypeError, ValueError):
        return None, rows

    for row in payload.get("AI") or []:
        if not isinstance(row, list) or len(row) < 5:
            continue
        try:
            area_type = int(row[0])
            x_coordinate = int(row[1])
            y_coordinate = int(row[2])
            gaa_value = int(row[4])
        except (TypeError, ValueError):
            continue

        if area_type != AREA_BARRON:
            continue

        current_level = gaa_level_from_value(packet_kid, gaa_value)
        if current_level is None:
            continue

        rows.append((packet_kid, x_coordinate, y_coordinate, int(current_level)))

    return packet_kid, rows


def process_live_message(decoded: str) -> None:
    if process_live_cra_response(decoded):
        return
    if process_live_cat_response(decoded):
        return
    parsed = parse_xt_packet(decoded.strip())
    if parsed and parsed.get("command") == "aci" and parsed.get("status") not in {None, "0", 0}:
        if process_pending_berimond_aci_error(parsed.get("status")):
            return
    if parsed and parsed.get("command") == "adi" and parsed.get("status") not in {None, "0", 0}:
        if process_pending_berimond_aci_error(parsed.get("status")):
            return
        if process_pending_storm_adi_error(parsed.get("status")):
            return

    payload = parse_live_payload(decoded, "gaa")
    exact_adi = False
    exact_aci = False
    if payload is not None and process_storm_gaa_payload(payload):
        return
    if payload is None:
        payload = parse_live_payload(decoded, "adi")
        exact_adi = True
    if payload is None:
        payload = parse_live_payload(decoded, "aci")
        exact_aci = True
    if payload is None:
        return

    if exact_aci:
        if process_pending_berimond_aci_payload(payload):
            return
    if exact_adi:
        if process_pending_berimond_aci_payload(payload):
            return
        if process_pending_storm_adi_payload(payload):
            return

    if exact_adi:
        row = adi_target_row(payload)
        packet_kid = row[0] if row is not None else None
        rows = [row] if row is not None else []
    elif exact_aci:
        packet_kid = int_or_none(payload.get("KID", (payload.get("gaa") or {}).get("KID")))
        rows = []
    else:
        packet_kid, rows = live_rbc_rows_from_payload(payload)
    if not rows:
        if packet_kid is not None and not (packet_kid == bot.BERIMOND_KID and not exact_adi):
            source = "aci" if exact_aci else ("adi" if exact_adi else "gaa")
            control_log(f"no_npc_rbc_or_exact_level packet_kid={packet_kid} source={source}")
        return

    rows_by_location = {
        (kingdom_id, x_coordinate, y_coordinate): (kingdom_id, x_coordinate, y_coordinate, current_level)
        for kingdom_id, x_coordinate, y_coordinate, current_level in rows
    }
    try:
        changed = upsert_rbc_rows(db(), list(rows_by_location.values()))
        db().commit()
        control_log(
            f"rbc_upsert packet_kid={packet_kid} source={'adi' if exact_adi else 'gaa'} "
            f"npc_rows={len(rows_by_location)} db_writes={changed}"
        )
    except Exception as exc:
        if db_conn is not None:
            db_conn.rollback()
        control_log(f"rbc_upsert_failed error={exc!r}")
        return

    if not exact_adi:
        return

    state = load_control()
    pending = state.get("pending")
    target = target_from_pending(pending)
    if not state.get("running") or not isinstance(pending, dict) or pending.get("kind") != "adi" or target is None:
        return
    if not same_target(payload, target):
        return

    exact_level = rows[0][3] if rows else None
    task = sands_task_by_name(pending.get("task_name"))
    if task is None:
        task = sands_task_by_name(target.get("task_name"))
    if task is None:
        task = sands_task_by_name("sand_rbc_level_61_crossbow")
    if task is None or not task.accepts_level(exact_level):
        bot.set_target_next_epoch(
            db(),
            int(target["id"]),
            bot.now_epoch() + random.uniform(*bot.TARGET_BAD_LEVEL_RETRY_RANGE),
            level=exact_level,
        )
        state["pending"] = None
        save_control(state)
        control_log(
            f"proxy_adi_skip target={target['x']}:{target['y']} "
            f"task={pending.get('task_name')} exact_level={exact_level}"
        )
        return

    target["current_level"] = int(exact_level)
    target["target_level"] = int(exact_level)
    target_lids = raw_target_available_lids(payload)
    lid = bot.choose_commander(db(), target_lids, target_lids, task.commander_lids)
    if lid is None:
        bot.restore_reserved_target(db(), target)
        next_wait = next_allowed_commander_wait(target_lids, task.commander_lids)
        if next_wait is None:
            pause_next_action(bot.TARGET_NO_LID_RETRY_RANGE)
        else:
            lower = max(30.0, min(float(next_wait) + 15.0, 180.0))
            upper = max(lower + 10.0, min(float(next_wait) + 45.0, 240.0))
            pause_next_action((lower, upper))
        state["pending"] = None
        save_control(state)
        task_lids_in_adi = sorted(lid for lid in task.commander_lids if lid in target_lids)
        control_log(
            f"proxy_adi_skip target={target['x']}:{target['y']} task={task.name} "
            f"reason=no_available_task_lid task_lids_in_adi={task_lids_in_adi} next_wait={next_wait}"
        )
        return

    queue_pending_cra(state, target, lid, task)


def handle_websocket_message(flow: http.HTTPFlow) -> None:
    global active_flow

    identifier = remember_websocket(flow)
    if not is_target_websocket(flow):
        return

    active_flow = flow
    msg = flow.websocket.messages[-1]
    direction = "CLIENT -> SERVER" if msg.from_client else "SERVER -> CLIENT"
    ts = datetime.now()
    decoded = decode_message_content(
        msg.content,
        is_text=msg.is_text,
        flow_identifier=identifier,
    )
    formatted = format_message_for_log(decoded)

    capture_file = open_capture_file(ts)
    capture_file.write(f"[{ts.strftime('%H:%M:%S.%f')[:-3]}] {direction}\n{formatted}\n---\n")
    capture_file.flush()

    if msg.from_client:
        remember_client_header(decoded)
        return

    if not msg.from_client:
        process_live_message(decoded)


def handle_websocket_start(flow: http.HTTPFlow) -> None:
    global active_flow

    identifier = remember_websocket(flow)
    if is_target_websocket(flow):
        active_flow = flow
        control_log(
            f"websocket_start id={identifier} selected=yes "
            f"target={TARGET_WEBSOCKET.name_value} url={flow.request.url}"
        )
    else:
        control_log(f"websocket_start id={identifier} selected=no url={flow.request.url}")


def handle_websocket_end(flow: http.HTTPFlow) -> None:
    global active_flow

    identifier = flow_id(flow)
    websocket_flows.pop(identifier, None)
    zlib_streams.pop(identifier, None)
    if flow is active_flow:
        active_flow = None
    control_log(f"websocket_end id={identifier} url={flow.request.url}")


class RbcProxyListener:
    def load(self, loader) -> None:
        global control_task
        CAPTURE_FOLDER.mkdir(parents=True, exist_ok=True)
        if control_task is None or control_task.done():
            control_task = asyncio.create_task(proxy_control_loop())
        control_log("rbc_proxy_listener_loaded receive_only=no controlled_by=proxy_control.json")

    def websocket_message(self, flow: http.HTTPFlow) -> None:
        handle_websocket_message(flow)

    def websocket_start(self, flow: http.HTTPFlow) -> None:
        handle_websocket_start(flow)

    def websocket_end(self, flow: http.HTTPFlow) -> None:
        handle_websocket_end(flow)

    def done(self) -> None:
        global current_file, db_conn, control_task
        if control_task is not None:
            control_task.cancel()
            control_task = None
        if current_file is not None:
            current_file.close()
            current_file = None
        if db_conn is not None:
            db_conn.close()
            db_conn = None
        control_log("rbc_proxy_listener_done")


addons = [RbcProxyListener()]
