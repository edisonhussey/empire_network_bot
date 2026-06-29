from __future__ import annotations

import json
import queue
import random
import re
import sys
import threading
import time
from collections import deque
from datetime import datetime
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT))

from empire.attacks.attack import Attack, side, wave


PROXY_SEND_FILE = REPO_ROOT / "captures" / "inject3_send.txt"
LATEST_SERVER_MESSAGES = Path(__file__).resolve().with_name("latest_5_server_messages")

OUTER_SERVER_HEADER = "EmpireEx_42"
OUTER_RBC_UNIT_ID = 2069
OUTER_RBC_X = 714
OUTER_RBC_Y = 787
SOURCE_X = 713
SOURCE_Y = 788
KINGDOM_ID = 0

SKIP_COUNT = 15
ATTACK_INTERVAL_SECONDS = 15
ATTACK_INTERVAL_RANDOM_SECONDS = 2.4
RETURN_SAFETY_SECONDS = 2
MAIN_LOOP_SLEEP_SECONDS = 0.25
COMMANDER_LORD_IDS = (0, 2, 3, 4)

OUTER_RBC_ATTACK = Attack(
    wave1=wave(
        middle=side(
            units=[(OUTER_RBC_UNIT_ID, 28)],
        )
    )
)

COMMANDER_STATE = {
    lord_id: {
        "available": True,
        "status": "available",
        "landing_epoch": 0.0,
        "return_epoch": 0.0,
        "landed": True,
        "march_id": None,
    }
    for lord_id in COMMANDER_LORD_IDS
}

march_id_to_lord_id: dict[int, int] = {}
pending_lord_ids: deque[int] = deque()
seen_message_files: set[Path] = set()
seen_march_ids: deque[int] = deque(maxlen=50)
last_attack_sent = 0.0
next_attack_allowed_at = 0.0
outbound_queue: queue.Queue[dict[str, Any]] = queue.Queue()
outbound_worker_thread: threading.Thread | None = None
outbound_lords: set[int] = set()
outbound_busy = False
skip_session_pending_or_active = False


def log(message: str) -> None:
    stamp = datetime.now().strftime("%H:%M:%S.%f")[:-3]
    print(f"[{stamp}] {message}", flush=True)


def append_to_proxy(packet: str) -> None:
    PROXY_SEND_FILE.parent.mkdir(parents=True, exist_ok=True)
    with PROXY_SEND_FILE.open("a", encoding="utf-8") as handle:
        handle.write(f"{packet}\n")


def skip() -> None:
    msg = (
        f'%xt%{OUTER_SERVER_HEADER}%msd%1%'
        f'{{"X":{OUTER_RBC_X},"Y":{OUTER_RBC_Y},"MID":-1,"NID":-1,"MST":"MS2","KID":"{KINGDOM_ID}"}}%'
    )
    append_to_proxy(msg)


def skip_burst_blocking() -> None:
    for _ in range(SKIP_COUNT):
        time.sleep(random.randrange(40, 100) / 100)
        skip()


def outbound_worker() -> None:
    global outbound_busy, skip_session_pending_or_active

    while True:
        job = outbound_queue.get()
        outbound_busy = True
        try:
            if job["kind"] == "attack":
                log(f"attack sent using LID={job['LID']}")
                append_to_proxy(create_attack_packet(LID=job["LID"]))
            elif job["kind"] == "skip":
                log(f"initiating skip session for MID={job['MID']} count={SKIP_COUNT}")
                skip_burst_blocking()
                log(f"skip session complete for MID={job['MID']}")
        finally:
            if job.get("LID") is not None:
                outbound_lords.discard(job["LID"])
            if job["kind"] == "skip":
                skip_session_pending_or_active = False
            outbound_busy = False
            outbound_queue.task_done()


def ensure_outbound_worker() -> None:
    global outbound_worker_thread

    if outbound_worker_thread is not None and outbound_worker_thread.is_alive():
        return
    outbound_worker_thread = threading.Thread(target=outbound_worker, daemon=True)
    outbound_worker_thread.start()


def create_attack_packet(
    *,
    LID: int,
    SX: int = SOURCE_X,
    SY: int = SOURCE_Y,
    TX: int = OUTER_RBC_X,
    TY: int = OUTER_RBC_Y,
    KID: int = KINGDOM_ID,
    attack: Attack = OUTER_RBC_ATTACK,
    server_header: str = OUTER_SERVER_HEADER,
) -> str:
    payload = {
        "SX": SX,
        "SY": SY,
        "TX": TX,
        "TY": TY,
        "KID": KID,
        "LID": LID,
        "WT": 0,
        "HBW": -1,
        "BPC": 0,
        "ATT": 0,
        "AV": 0,
        "LP": 0,
        "FC": 0,
        "PTT": 1,
        "SD": 0,
        "ICA": 0,
        "CD": 99,
        "A": attack.to_payload(),
        "BKS": [],
        "AST": [-1, -1, -1],
        "RW": [[-1, 0] for _ in range(8)],
        "ASCT": 0,
    }
    json_payload = json.dumps(payload, separators=(",", ":"))
    return f"%xt%{server_header}%cra%1%{json_payload}%"


def send_attack(LID: int) -> str:
    if LID in locked_lord_ids():
        raise RuntimeError(f"LID {LID} is already locked and cannot be queued again")

    msg = create_attack_packet(LID=LID)
    ensure_outbound_worker()
    COMMANDER_STATE.setdefault(
        LID,
        {
            "available": True,
            "status": "available",
            "landing_epoch": 0.0,
            "return_epoch": 0.0,
            "landed": True,
            "march_id": None,
        },
    )
    COMMANDER_STATE[LID].update(
        {
            "available": False,
            "status": "pending_ack",
            "landing_epoch": float("inf"),
            "return_epoch": float("inf"),
            "landed": False,
            "march_id": None,
        }
    )
    pending_lord_ids.append(LID)
    outbound_lords.add(LID)
    outbound_queue.put({"kind": "attack", "LID": LID})
    log(f"attack queued using LID={LID}")
    return msg


def queue_skip_burst(march_id: int) -> None:
    global skip_session_pending_or_active

    if skip_session_pending_or_active:
        log(f"skip session already active; coalesced landing MID={march_id}")
        return
    skip_session_pending_or_active = True
    ensure_outbound_worker()
    outbound_queue.put({"kind": "skip", "MID": march_id})


def next_attack_delay() -> float:
    return ATTACK_INTERVAL_SECONDS + random.random() * ATTACK_INTERVAL_RANDOM_SECONDS


def latest_message_files() -> list[Path]:
    if not LATEST_SERVER_MESSAGES.exists():
        return []
    return sorted(LATEST_SERVER_MESSAGES.glob("*.txt"))


def file_epoch(path: Path) -> float:
    try:
        stamp = path.stem
        return datetime.strptime(stamp, "%Y%m%d_%H%M%S_%f").timestamp()
    except ValueError:
        return path.stat().st_mtime


def extract_raw_packet(text: str) -> str | None:
    match = re.search(r"(?ms)^\s*raw:\s*\n(?P<packet>%xt%[^\n]+%)\s*\n---", text)
    if match:
        return match.group("packet").strip()
    match = re.search(r"(?m)(%xt%[^\n]+%)", text)
    if match:
        return match.group(1).strip()
    return None


def parse_xt_packet(packet: str) -> dict[str, Any] | None:
    fields = packet.strip().strip("%").split("%")
    if len(fields) < 5 or fields[0] != "xt":
        return None
    if fields[1].startswith("EmpireEx_"):
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
        payload_text = fields[4]
    try:
        payload = json.loads(payload_text)
    except json.JSONDecodeError:
        payload = payload_text
    return {
        "server_header": server_header,
        "command": command,
        "request_id": request_id,
        "status": status,
        "payload": payload,
        "raw": packet,
    }


def parse_message_file(path: Path) -> dict[str, Any] | None:
    text = path.read_text(encoding="utf-8", errors="replace")
    packet = extract_raw_packet(text)
    if packet is None:
        return None
    parsed = parse_xt_packet(packet)
    if parsed is None:
        return None
    parsed["path"] = path
    parsed["epoch"] = file_epoch(path)
    return parsed


def remember_march(march_id: int, lord_id: int, travel_seconds: int, seen_at: float) -> bool:
    if march_id in seen_march_ids:
        return False
    seen_march_ids.append(march_id)
    march_id_to_lord_id[march_id] = lord_id
    COMMANDER_STATE.setdefault(
        lord_id,
        {
            "available": True,
            "status": "available",
            "landing_epoch": 0.0,
            "return_epoch": 0.0,
            "landed": True,
            "march_id": None,
        },
    )
    COMMANDER_STATE[lord_id].update(
        {
            "available": False,
            "status": "marching",
            "landing_epoch": seen_at + travel_seconds,
            "return_epoch": float("inf"),
            "landed": False,
            "march_id": march_id,
        }
    )
    return True


def locked_lord_ids() -> set[int]:
    locked = {
        lord_id
        for lord_id, state in COMMANDER_STATE.items()
        if not state.get("available") or state.get("status") != "available"
    }
    locked.update(outbound_lords)
    locked.update(pending_lord_ids)
    locked.update(march_id_to_lord_id.values())
    return locked


def available_lord_ids() -> list[int]:
    locked = locked_lord_ids()
    return [
        lord_id
        for lord_id in COMMANDER_LORD_IDS
        if lord_id not in locked
        and COMMANDER_STATE.get(lord_id, {}).get("available") is True
        and COMMANDER_STATE.get(lord_id, {}).get("status") == "available"
    ]


def remove_pending_lord_id(lord_id: int) -> None:
    try:
        pending_lord_ids.remove(lord_id)
    except ValueError:
        pass


def next_pending_lord_id() -> int | None:
    while pending_lord_ids:
        lord_id = pending_lord_ids.popleft()
        if lord_id in COMMANDER_STATE and not COMMANDER_STATE[lord_id]["available"]:
            return lord_id
    return None


def is_outer_rbc_hit(movement: dict[str, Any], attack: dict[str, Any]) -> bool:
    source_area = movement.get("SA")
    if not isinstance(source_area, list) or len(source_area) < 3:
        return False
    if int(source_area[1]) != OUTER_RBC_X or int(source_area[2]) != OUTER_RBC_Y:
        return False

    remaining_units = attack.get("A") or []
    return any(
        isinstance(row, list)
        and len(row) >= 1
        and int(row[0]) == OUTER_RBC_UNIT_ID
        for row in remaining_units
    )


def earned_food_and_coins(attack: dict[str, Any]) -> bool:
    rewards = attack.get("G") or []
    if not isinstance(rewards, list):
        return False

    reward_totals = {}
    for reward in rewards:
        if not isinstance(reward, list) or len(reward) < 2:
            continue
        reward_type, amount = reward[0], reward[1]
        try:
            reward_totals[str(reward_type)] = int(amount)
        except (TypeError, ValueError):
            continue

    return reward_totals.get("F", 0) > 0 and reward_totals.get("C1", 0) > 0


def process_cat(parsed: dict[str, Any]) -> None:
    payload = parsed.get("payload")
    if not isinstance(payload, dict):
        return
    attack = payload.get("A")
    if not isinstance(attack, dict):
        return
    movement = attack.get("M")
    if not isinstance(movement, dict):
        return
    if not is_outer_rbc_hit(movement, attack):
        return
    lord = attack.get("UM", {}).get("L", {})
    if not isinstance(lord, dict):
        lord = {}

    march_id = movement.get("MID")
    lord_id = lord.get("ID")
    travel_seconds = movement.get("TT")
    if march_id is None or travel_seconds is None:
        return
    if lord_id is None:
        lord_id = next_pending_lord_id()
    else:
        lord_id = int(lord_id)
        remove_pending_lord_id(lord_id)
    if lord_id is None:
        return
    is_new_march = remember_march(
        march_id=int(march_id),
        lord_id=lord_id,
        travel_seconds=int(travel_seconds),
        seen_at=float(parsed["epoch"]),
    )
    if is_new_march:
        log(
            "landing detected "
            f"MID={int(march_id)} LID={lord_id} "
            f"travel_seconds={int(travel_seconds)}"
        )
        if earned_food_and_coins(attack):
            queue_skip_burst(int(march_id))
        else:
            log(f"skip not queued for MID={int(march_id)} reason=no_food_and_coins")


def process_dms(parsed: dict[str, Any]) -> None:
    payload = parsed.get("payload")
    if not isinstance(payload, dict):
        return
    mids = payload.get("MID") or []
    for march_id in mids:
        lord_id = march_id_to_lord_id.pop(int(march_id), None)
        if lord_id is not None and lord_id in COMMANDER_STATE:
            COMMANDER_STATE[lord_id].update(
                {
                    "available": True,
                    "status": "available",
                    "landed": True,
                    "landing_epoch": 0.0,
                    "return_epoch": 0.0,
                    "march_id": None,
                }
            )


def update_commander_state() -> dict[int, dict[str, Any]]:
    for path in latest_message_files():
        if path in seen_message_files:
            continue
        parsed = parse_message_file(path)
        seen_message_files.add(path)
        if parsed is None:
            continue
        if parsed["command"] in {"cat", "mcm"}:
            process_cat(parsed)
        elif parsed["command"] == "dms":
            process_dms(parsed)

    return COMMANDER_STATE


def reset_runtime_state() -> None:
    global last_attack_sent, next_attack_allowed_at, skip_session_pending_or_active

    march_id_to_lord_id.clear()
    pending_lord_ids.clear()
    seen_march_ids.clear()
    seen_message_files.clear()
    seen_message_files.update(latest_message_files())
    last_attack_sent = 0.0
    next_attack_allowed_at = 0.0
    skip_session_pending_or_active = False

    for lord_id in COMMANDER_LORD_IDS:
        COMMANDER_STATE[lord_id] = {
            "available": True,
            "status": "available",
            "landing_epoch": 0.0,
            "return_epoch": 0.0,
            "landed": True,
            "march_id": None,
        }
    log(f"runtime state reset; assuming free LIDs={COMMANDER_LORD_IDS}")


def next_attack() -> int | None:
    global last_attack_sent, next_attack_allowed_at

    now = time.time()
    if now < next_attack_allowed_at:
        return None
    if outbound_busy or not outbound_queue.empty():
        return None
    lord_ids = available_lord_ids()
    if not lord_ids:
        return None
    lord_id = lord_ids[0]
    send_attack(lord_id)
    last_attack_sent = now
    next_attack_allowed_at = now + next_attack_delay()
    return lord_id


def run() -> None:
    reset_runtime_state()
    while True:
        update_commander_state()
        next_attack()
        time.sleep(MAIN_LOOP_SLEEP_SECONDS)


if __name__ == "__main__":
    print(create_attack_packet(LID=0))
    run()
