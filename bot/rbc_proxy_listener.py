from __future__ import annotations

import asyncio
import base64
import json
import os
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
        if (parent / "empire").is_dir():
            return parent
    return path.parents[2]


REPO_ROOT = find_repo_root(Path(__file__).resolve())
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from empire.bot.populate_database_rbc import adi_target_row, gaa_level_from_value, upsert_rbc_rows
from empire.bot import bot
from empire.bot.test_psql_connection import connect, read_connection_config
from empire.network_sender.websockets import OUTER_WEBSOCKET
from empire.sand_rbc_farm.main import AREA_BARRON, parse_xt_packet


HERE = REPO_ROOT / "empire" / "bot"
CAPTURE_FOLDER = HERE / "logs"
CONTROL_LOG = HERE / "rbc_proxy_listener.log"
CONTROL_FILE = HERE / "proxy_control.json"
ROLL_SECONDS = 10
CONTROL_POLL_SECONDS = 1.0
NO_TARGET_RETRY_RANGE = (55.0, 145.0)
TARGET_WEBSOCKET = OUTER_WEBSOCKET

current_file = None
last_roll = datetime.now()
active_flow = None
websocket_flows = {}
zlib_streams = {}
db_conn = None
control_task = None
next_proxy_action_epoch = 0.0


def control_log(message: str) -> None:
    ts = datetime.now().strftime("%Y-%m-%d %H:%M:%S.%f")[:-3]
    with CONTROL_LOG.open("a", encoding="utf-8") as handle:
        handle.write(f"[{ts}] {message}\n")
        handle.flush()


def load_control() -> dict:
    if not CONTROL_FILE.exists():
        return {"running": False, "max_attacks": 0, "attacks_sent": 0, "pending": None}
    try:
        data = json.loads(CONTROL_FILE.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {"running": False, "max_attacks": 0, "attacks_sent": 0, "pending": None}
    return data if isinstance(data, dict) else {"running": False, "max_attacks": 0, "attacks_sent": 0, "pending": None}


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
    return db_conn


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


def xt_packet(command: str, payload: dict, *, request_id: int = 1) -> str:
    return "%xt%{}%{}%{}%{}%".format(
        bot.farm.SAND_SERVER_HEADER,
        command,
        int(request_id),
        json.dumps(payload, separators=(",", ":")),
    )


def create_adi_packet(target: dict) -> str:
    return xt_packet(
        "adi",
        {
            "SX": bot.farm.SOURCE_X,
            "SY": bot.farm.SOURCE_Y,
            "TX": int(target["x"]),
            "TY": int(target["y"]),
            "KID": bot.SANDS_KID,
        },
    )


def create_cra_packet(target: dict, lid: int) -> str:
    return xt_packet("cra", bot.build_attack_payload(target, lid))


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


def pause_next_action(seconds_range: tuple[float, float]) -> None:
    global next_proxy_action_epoch
    next_proxy_action_epoch = time.time() + random.uniform(*seconds_range)


def stop_control(state: dict, reason: str) -> None:
    state["running"] = False
    state["pending"] = None
    state["stopped_at"] = bot.now_epoch()
    state["stop_reason"] = reason
    save_control(state)
    control_log(f"proxy_stopped reason={reason}")


def increment_attacks_sent(state: dict) -> None:
    attacks_sent = int(state.get("attacks_sent", 0) or 0) + 1
    state["attacks_sent"] = attacks_sent
    max_attacks = int(state.get("max_attacks", 0) or 0)
    if max_attacks and attacks_sent >= max_attacks:
        stop_control(state, f"max_attacks count={attacks_sent}")
    else:
        save_control(state)


def queue_pending_cra(state: dict, target: dict, lid: int) -> None:
    due_at = time.time() + random.uniform(*bot.ADI_TO_CRA_DELAY_RANGE)
    payload = bot.build_attack_payload(target, lid)
    state["pending"] = {
        "kind": "cra",
        "target": target,
        "lid": int(lid),
        "due_at": due_at,
        "army_count": troop_count_from_payload(payload),
    }
    save_control(state)
    control_log(
        f"proxy_cra_pending target={target['x']}:{target['y']} kid={target['kingdom_id']} "
        f"level={bot.TARGET_LEVEL} lid={lid} army_count={state['pending']['army_count']} "
        f"due_in={due_at - time.time():.1f}s"
    )


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
            WHERE kingdom_id = %s AND x_coordinate = %s AND y_coordinate = %s
            """,
            (bot.SANDS_KID, x_coordinate, y_coordinate),
        )
        row = cur.fetchone()
    return int(row[0]) if row is not None else None


def shorten_commander_return(lid: int, march_id: int | None, rbc_id: int | None, return_epoch: float) -> None:
    with db().cursor() as cur:
        cur.execute("SELECT available_after FROM commander_state WHERE lord_id = %s", (int(lid),))
        row = cur.fetchone()
        if row is None:
            return
        current_available = int(row[0] or 0)
        next_available = int(return_epoch)
        if current_available > bot.now_epoch():
            next_available = min(current_available, next_available)
        cur.execute(
            """
            UPDATE commander_state
            SET available_after = %s,
                status = 'returning',
                march_id = %s,
                target_rbc_id = COALESCE(%s, target_rbc_id),
                updated_at = %s
            WHERE lord_id = %s
            """,
            (next_available, march_id, rbc_id, bot.now_epoch(), int(lid)),
        )
    db().commit()


def next_allowed_commander_wait(target_lids: set[int]) -> int | None:
    allowed = [lid for lid in bot.FIRST_13_COMMANDER_LIDS if lid in target_lids]
    if not allowed:
        return None
    with db().cursor() as cur:
        cur.execute(
            """
            SELECT MIN(available_after)
            FROM commander_state
            WHERE lord_id = ANY(%s)
            """,
            (allowed,),
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

    state = load_control()
    last_cra = state.get("last_cra") if isinstance(state.get("last_cra"), dict) else {}
    target = last_cra_target(state)
    lid = last_cra.get("lid")
    status = parsed.get("status")

    if status not in (None, "0", 0):
        if target is not None:
            bot.release_target(db(), int(target["id"]), bot.TARGET_ERROR_RETRY_RANGE)
        if lid is not None:
            bot.release_commander(
                db(),
                int(lid),
                available_after=bot.now_epoch() + bot.HEURISTIC_RETURN_SECONDS + random.uniform(*bot.COMMANDER_RETURN_HOLD_RANGE),
                status=f"cra_status_{status}",
            )
        state["last_cra"] = None
        save_control(state)
        control_log(
            f"proxy_cra_error status={status} "
            f"target={target['x']}:{target['y']}" if target is not None else f"proxy_cra_error status={status}"
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
            "kingdom_id": bot.SANDS_KID,
            "x": x_coordinate,
            "y": y_coordinate,
        }

    if target is None or lid is None or march_id is None:
        control_log(
            f"proxy_cra_ack_unparsed reason=missing_ids mid={march_id} lid={lid} target_known={target is not None}"
        )
        return True

    sent_at = int(last_cra.get("sent_at") or bot.now_epoch())
    commander_available = sent_at + travel_seconds + bot.HEURISTIC_RETURN_SECONDS + random.uniform(*bot.COMMANDER_RETURN_HOLD_RANGE)
    if int(target.get("id", 0) or 0):
        bot.mark_commander_outbound(db(), int(lid), int(target["id"]), int(march_id), commander_available)
        bot.set_target_next_epoch(db(), int(target["id"]), bot.target_next_epoch(sent_at, travel_seconds), level=bot.TARGET_LEVEL)
        bot.record_attack(db(), target, int(march_id), sent_at, travel_seconds)
    state["last_cra"] = None
    save_control(state)
    control_log(
        f"proxy_cra_ack target={target['x']}:{target['y']} kid={target['kingdom_id']} "
        f"level={bot.TARGET_LEVEL} lid={lid} mid={march_id} "
        f"army_count={last_cra.get('army_count', 'unknown')} travel_duration={travel_seconds} "
        f"return_duration={bot.HEURISTIC_RETURN_SECONDS}"
    )
    return True


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
    if lid not in bot.FIRST_13_COMMANDER_LIDS:
        return False

    source_area = movement_area(movement, "SA")
    rbc_id = rbc_id_for_area(source_area)
    march_id = movement.get("MID")
    march_id_int = int(march_id) if march_id is not None else None
    return_seconds = int(float(movement.get("TT", 0) or 0))
    return_epoch = bot.now_epoch() + return_seconds + random.uniform(*bot.COMMANDER_RETURN_HOLD_RANGE)
    shorten_commander_return(lid, march_id_int, rbc_id, return_epoch)
    target_text = f"{source_area[1]}:{source_area[2]}" if source_area is not None else "unknown"
    control_log(
        f"proxy_cat_return target={target_text} kid={bot.SANDS_KID} lid={lid} mid={march_id_int} "
        f"return_duration={return_seconds} available_after={int(return_epoch)}"
    )
    return True


async def proxy_control_loop() -> None:
    global next_proxy_action_epoch

    while True:
        try:
            state = load_control()
            if not state.get("running"):
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue
            if int(state.get("max_attacks", 0) or 0) and int(state.get("attacks_sent", 0) or 0) >= int(state.get("max_attacks", 0) or 0):
                stop_control(state, "max_attacks_already_reached")
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue
            if time.time() < next_proxy_action_epoch:
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue
            if not is_open_websocket(active_flow):
                control_log("proxy_wait reason=no_active_websocket")
                pause_next_action((15.0, 35.0))
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue

            pending = state.get("pending")
            if isinstance(pending, dict) and pending.get("kind") == "cra":
                due_at = float(pending.get("due_at", 0.0) or 0.0)
                if time.time() < due_at:
                    await asyncio.sleep(CONTROL_POLL_SECONDS)
                    continue
                target = target_from_pending(pending)
                lid = pending.get("lid")
                if target is None or lid is None:
                    state["pending"] = None
                    save_control(state)
                    continue
                attack_payload = bot.build_attack_payload(target, int(lid))
                packet = xt_packet("cra", attack_payload)
                army_count = troop_count_from_payload(attack_payload)
                sent_at = bot.now_epoch()
                commander_next = sent_at + bot.HEURISTIC_RETURN_SECONDS + random.uniform(*bot.COMMANDER_RETURN_HOLD_RANGE)
                bot.mark_commander_pending(db(), int(lid), int(target["id"]), commander_next)
                inject_packet(packet)
                state["pending"] = None
                state["last_cra"] = {
                    "target": target,
                    "lid": int(lid),
                    "sent_at": sent_at,
                    "army_count": army_count,
                }
                control_log(
                    f"proxy_cra_sent target={target['x']}:{target['y']} kid={target['kingdom_id']} "
                    f"level={bot.TARGET_LEVEL} lid={lid} mid=pending "
                    f"army_count={army_count} travel_duration=pending"
                )
                increment_attacks_sent(state)
                pause_next_action(bot.REQUEST_INTERVAL_RANGE)
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue

            if pending:
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue

            target = bot.reserve_target(db())
            if target is None:
                control_log("proxy_idle reason=no_level_61_sands_target")
                pause_next_action(NO_TARGET_RETRY_RANGE)
                await asyncio.sleep(CONTROL_POLL_SECONDS)
                continue
            inject_packet(create_adi_packet(target))
            state["pending"] = {
                "kind": "adi",
                "target": target,
                "sent_at": bot.now_epoch(),
            }
            save_control(state)
            control_log(
                f"proxy_adi_sent target={target['x']}:{target['y']} kid={target['kingdom_id']} "
                f"expected_level={bot.TARGET_LEVEL}"
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

    payload = parse_live_payload(decoded, "gaa")
    exact_adi = False
    if payload is None:
        payload = parse_live_payload(decoded, "adi")
        exact_adi = True
    if payload is None:
        return

    if exact_adi:
        row = adi_target_row(payload)
        packet_kid = row[0] if row is not None else None
        rows = [row] if row is not None else []
    else:
        packet_kid, rows = live_rbc_rows_from_payload(payload)
    if not rows:
        if packet_kid is not None:
            control_log(f"no_npc_rbc_or_exact_level packet_kid={packet_kid} source={'adi' if exact_adi else 'gaa'}")
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
    if exact_level != bot.TARGET_LEVEL:
        bot.set_target_next_epoch(
            db(),
            int(target["id"]),
            bot.now_epoch() + random.uniform(*bot.TARGET_BAD_LEVEL_RETRY_RANGE),
            level=exact_level,
        )
        state["pending"] = None
        save_control(state)
        control_log(f"proxy_adi_skip target={target['x']}:{target['y']} exact_level={exact_level}")
        return

    target_lids = raw_target_available_lids(payload)
    lid = bot.choose_commander(db(), target_lids, target_lids)
    if lid is None:
        bot.restore_reserved_target(db(), target)
        next_wait = next_allowed_commander_wait(target_lids)
        if next_wait is None:
            pause_next_action(bot.TARGET_NO_LID_RETRY_RANGE)
        else:
            lower = max(30.0, min(float(next_wait) + 15.0, 180.0))
            upper = max(lower + 10.0, min(float(next_wait) + 45.0, 240.0))
            pause_next_action((lower, upper))
        state["pending"] = None
        save_control(state)
        first_in_adi = sorted(lid for lid in bot.FIRST_13_COMMANDER_LIDS if lid in target_lids)
        control_log(
            f"proxy_adi_skip target={target['x']}:{target['y']} reason=no_available_first_13_lid "
            f"first_in_adi={first_in_adi} next_wait={next_wait}"
        )
        return

    queue_pending_cra(state, target, lid)


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
