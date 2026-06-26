#!/usr/bin/env python3
"""Repeat Berimond attacks from a captured `cra` packet.

This manager is deliberately small and local to this repo. It imports the known
socket/session helpers from the read-only scan_coordinates tree, but all capture,
state, and login-detail handling lives here.
"""

from __future__ import annotations

import argparse
import configparser
import json
import math
import os
import queue
import random
import re
import sys
import tempfile
import time
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_SCAN_ROOT = Path.home() / "Desktop" / "scan_coordinates"
DEFAULT_ACCOUNT_CONFIG = DEFAULT_SCAN_ROOT / "ventrilo.ini"
DEFAULT_LOGIN_DETAILS = REPO_ROOT / "login_details.txt"
DEFAULT_LOGIN_CAPTURE = REPO_ROOT / "login_captures" / "furox_login.txt"
DEFAULT_CAPTURE = Path(__file__).resolve().with_name("berimond_attack_capture.json")
DEFAULT_STATE = Path(__file__).resolve().with_name("berimond_attack_state.json")
DEFAULT_SERVER_URL = "wss://ep-live-mz-int1-sk1-gb1-game.goodgamestudios.com/"

BERIMOND = 10
GREEN = 0
BERIMOND_EVENT_ID = 3


def add_scan_paths(scan_root: Path) -> None:
    for path in (scan_root, scan_root / "pygge_repo"):
        if str(path) not in sys.path:
            sys.path.insert(0, str(path))


def now_iso() -> str:
    return datetime.now().astimezone().isoformat(timespec="seconds")


def log(message: str, *, error: bool = False) -> None:
    print(
        f"[berimond] {message} timestamp={now_iso()}",
        file=sys.stderr if error else sys.stdout,
        flush=True,
    )


def random_delay(
    label: str,
    *,
    minimum: float,
    maximum: float,
    jitter: float = 0.0,
) -> None:
    delay = random.uniform(minimum, maximum)
    if jitter:
        delay += random.uniform(-jitter, jitter)
    delay = max(0.0, delay)
    log(f"{label}_wait={delay:.1f}s")
    time.sleep(delay)


def load_json(path: Path, default: Any) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return default


def save_json(path: Path, data: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(data, indent=2), encoding="utf-8")
    tmp.replace(path)


def read_login_details(path: Path) -> dict[str, str]:
    text = path.read_text(encoding="utf-8")
    username = re.search(r"(?im)^\s*username\s*:\s*(.+?)\s*$", text)
    password = re.search(r"(?im)^\s*password\s*:\s*(.+?)\s*$", text)
    if not username or not password:
        raise ValueError(f"{path} must contain username: and password: lines")
    return {
        "username": username.group(1).strip(),
        "password": password.group(1).strip(),
    }


def read_login_capture(path: Path) -> dict[str, Any]:
    text = path.read_text(encoding="utf-8")
    match = re.search(r"%xt%([^%]+)%lli%1%(.+?)%", text, flags=re.S)
    if not match:
        raise ValueError(f"{path} does not contain an lli capture")
    payload = json.loads(match.group(2))
    return {
        "server_header": match.group(1),
        "CONM": payload.get("CONM"),
        "RTM": payload.get("RTM"),
        "SID": payload.get("SID", 9),
        "PLFID": payload.get("PLFID", 1),
        "account_id": str(payload.get("AID", "")).strip(),
        "RCT": str(payload.get("RCT", "")).strip(),
    }


def write_runtime_config(
    *,
    server_url: str,
    login_details: Path,
    login_capture: Path,
) -> tempfile.NamedTemporaryFile:
    details = read_login_details(login_details)
    capture = read_login_capture(login_capture)

    parser = configparser.ConfigParser()
    parser["gge"] = {
        "server_url": server_url,
        "server_header": capture["server_header"],
        "username": details["username"],
        "password": details["password"],
        "account_id": capture["account_id"],
        "CONM": str(capture["CONM"]),
        "RTM": str(capture["RTM"]),
        "SID": str(capture["SID"]),
        "PLFID": str(capture["PLFID"]),
    }
    if capture.get("RCT"):
        parser["gge"]["RCT"] = capture["RCT"]

    handle = tempfile.NamedTemporaryFile("w", suffix=".ini", delete=False)
    with handle:
        parser.write(handle)
    return handle


@dataclass
class ActiveAttack:
    lord_id: int
    target_x: int
    target_y: int
    sent_at: float
    release_at: float
    march_id: int | None = None
    travel_seconds: int | None = None


@dataclass
class RuntimeEvents:
    messages: queue.Queue[dict[str, Any]] = field(default_factory=queue.Queue)
    log_messages: bool = True

    def on_message(self, ws, raw_message: str) -> None:
        try:
            parsed = ws.parse_response(raw_message)
        except Exception as exc:
            self.messages.put({"kind": "parse_error", "error": str(exc)})
            if self.log_messages:
                log(f"recv parse_error error={exc}", error=True)
            return
        payload = parsed.get("payload", {})
        command = payload.get("command")
        if self.log_messages:
            log(f"recv {summarize_response(parsed)}")
        if command in {"cat", "dms", "cra"}:
            self.messages.put(parsed)


def summarize_response(response: dict[str, Any]) -> str:
    payload = response.get("payload", {})
    if response.get("type") != "json":
        return f"type={response.get('type')} payload_keys={sorted(payload.keys())}"

    command = payload.get("command")
    status = payload.get("status")
    data = payload.get("data")
    prefix = f"cmd={command} status={status}"
    if isinstance(data, dict):
        if command in {"cra", "cat"}:
            mid, tt = attack_response_movement(response)
            movement = data.get("AAM", {}).get("M", {})
            target = movement.get("SA") or movement.get("TA")
            return f"{prefix} march_id={mid} travel_seconds={tt} target={target}"
        if command == "dms":
            return f"{prefix} mids={data.get('MID')}"
        if command == "gaa":
            return (
                f"{prefix} kid={data.get('KID')} "
                f"areas={len(data.get('AI') or [])} owners={len(data.get('OI') or [])}"
            )
        if command == "adi":
            gli = data.get("gli") or {}
            return (
                f"{prefix} commanders={len(gli.get('C') or [])} "
                f"keys={','.join(sorted(data.keys())[:8])}"
            )
        if command == "gli":
            return f"{prefix} commanders={len(data.get('C') or [])}"
        if command == "gam":
            return f"{prefix} keys={','.join(sorted(data.keys())[:8])}"
        if command == "gcl":
            kingdoms = data.get("C") or []
            castle_count = 0
            for kingdom in kingdoms:
                castle_count += len(kingdom.get("AI") or [])
            return f"{prefix} kingdoms={len(kingdoms)} castles={castle_count}"
        return f"{prefix} keys={','.join(sorted(data.keys())[:8])}"
    if isinstance(data, list):
        return f"{prefix} list_len={len(data)}"
    if data is None:
        return prefix
    return f"{prefix} data={str(data)[:120]!r}"


class AttackAttemptError(RuntimeError):
    def __init__(
        self,
        message: str,
        *,
        lord_id: int | None = None,
        target: tuple[int, int] | None = None,
    ) -> None:
        super().__init__(message)
        self.lord_id = lord_id
        self.target = target


def movement_lord_ids(response: dict | bool | None) -> set[int] | None:
    if not isinstance(response, dict):
        return None
    data = response.get("payload", {}).get("data") or {}
    found: set[int] = set()

    def walk(value: Any) -> None:
        if isinstance(value, dict):
            for key, child in value.items():
                if key in {"LID", "WID"}:
                    try:
                        found.add(int(child))
                    except (TypeError, ValueError):
                        pass
                walk(child)
        elif isinstance(value, list):
            for child in value:
                walk(child)

    walk(data)
    return found


def prune_active(attacks: list[ActiveAttack], socket) -> list[ActiveAttack]:
    now = time.time()
    return [attack for attack in attacks if attack.release_at > now]


def load_capture(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    payload = data.get("outgoing", data)
    required = {"SX", "SY", "TX", "TY", "KID", "LID", "A", "RW"}
    missing = sorted(required - set(payload))
    if missing:
        raise ValueError(f"{path} missing capture fields: {', '.join(missing)}")
    return payload


def extract_castles(response: dict) -> list[dict[str, int]]:
    data = response.get("payload", {}).get("data") or {}
    rows: list[dict[str, int]] = []
    for kingdom_block in data.get("C", []):
        kid = int(kingdom_block.get("KID", -1))
        for area in kingdom_block.get("AI", []):
            coords = area.get("AI") if isinstance(area, dict) else area
            if isinstance(coords, list) and len(coords) >= 4:
                rows.append(
                    {
                        "kingdom": kid,
                        "type": int(coords[0]),
                        "x": int(coords[1]),
                        "y": int(coords[2]),
                        "castle_id": int(coords[3]),
                    }
                )
    return rows


def choose_source(socket, capture: dict[str, Any]) -> tuple[int, int, int]:
    captured_sx = int(capture["SX"])
    captured_sy = int(capture["SY"])
    captured_kid = int(capture.get("KID", BERIMOND))
    castles = extract_castles(socket.get_castles())
    for row in castles:
        if (
            row["kingdom"] == captured_kid
            and row["x"] == captured_sx
            and row["y"] == captured_sy
        ):
            return row["x"], row["y"], row["castle_id"]
    return captured_sx, captured_sy, -1


def distance(ax: int, ay: int, bx: int, by: int) -> float:
    return math.hypot(ax - bx, ay - by)


def extract_npc_targets(response: dict | bool, *, npc_type: int) -> list[tuple[int, int, int]]:
    if not isinstance(response, dict):
        return []
    data = response.get("payload", {}).get("data") or {}
    targets: list[tuple[int, int, int]] = []
    for row in data.get("AI", []):
        if not isinstance(row, list) or len(row) < 3:
            continue
        try:
            area_type = int(row[0])
            x = int(row[1])
            y = int(row[2])
            level = int(row[4]) if len(row) > 4 else -1
        except (TypeError, ValueError):
            continue
        if area_type == npc_type:
            targets.append((x, y, level))
    return targets


def find_target(
    socket,
    *,
    sx: int,
    sy: int,
    kingdom: int,
    npc_type: int,
    scan_radius: int,
    scan_hint_dx: int,
    scan_hint_dy: int,
    scan_delay_min: float,
    scan_delay_max: float,
    scan_delay_jitter: float,
    rejected: set[tuple[int, int]],
) -> tuple[int, int, int]:
    hint_x = sx + scan_hint_dx
    hint_y = sy + scan_hint_dy
    offsets: list[tuple[int, int]] = []
    for dx in range(-scan_radius, scan_radius + 1, 13):
        for dy in range(-scan_radius, scan_radius + 1, 13):
            if distance(0, 0, dx, dy) <= scan_radius:
                offsets.append((dx, dy))

    # Bias toward the likely watchtower area, but add noise so requests do not
    # look like a perfectly linear sweep.
    offsets.sort(
        key=lambda item: (
            distance(sx + item[0], sy + item[1], hint_x, hint_y)
            + random.uniform(-18.0, 18.0)
        )
    )

    checked = 0
    for dx, dy in offsets:
        ax = max(0, sx + dx - 6)
        ay = max(0, sy + dy - 6)
        checked += 1
        random_delay(
            "scan_chunk",
            minimum=scan_delay_min,
            maximum=scan_delay_max,
            jitter=scan_delay_jitter,
        )
        log(
            f"scan_request chunk=({ax},{ay}) checked={checked}/{len(offsets)} "
            f"hint=({hint_x},{hint_y})"
        )
        response = socket.get_map_chunk(kingdom, ax, ay, quiet=True)
        for tx, ty, level in extract_npc_targets(response, npc_type=npc_type):
            if (tx, ty) not in rejected:
                log(
                    f"scan_found target=({tx},{ty}) level={level} "
                    f"distance={distance(sx, sy, tx, ty):.1f} chunks_checked={checked}"
                )
                return tx, ty, level
    raise RuntimeError(
        f"no npc_type={npc_type} targets found within radius={scan_radius} near ({sx},{sy})"
    )


def open_berimond(socket, args: argparse.Namespace | None = None) -> None:
    for command, payload, response_command in (
        ("klh", {}, "klh"),
        ("pep", {"EID": BERIMOND_EVENT_ID}, "pep"),
    ):
        try:
            if args is not None:
                random_delay(
                    f"step_{command}",
                    minimum=args.step_delay_min,
                    maximum=args.step_delay_max,
                    jitter=args.step_jitter,
                )
            socket.send_json_command(command, payload)
            socket.wait_for_json_response(response_command, timeout=8)
        except Exception as exc:
            log(f"open_berimond step={command} skipped error={exc}", error=True)


def resolve_lord(socket, target_info: dict, active: list[ActiveAttack]) -> int:
    from event_worker.lords import resolve_available_lord_id

    excluded = {attack.lord_id for attack in active}
    return resolve_available_lord_id(
        socket.get_lords(),
        target_info,
        excluded_lord_ids=excluded,
    )


def build_payload(
    capture: dict[str, Any],
    *,
    sx: int,
    sy: int,
    tx: int,
    ty: int,
    lord_id: int,
) -> dict[str, Any]:
    payload = json.loads(json.dumps(capture))
    payload.update({"SX": sx, "SY": sy, "TX": tx, "TY": ty, "KID": BERIMOND, "LID": lord_id})
    return payload


def attack_response_movement(response: dict) -> tuple[int | None, int | None]:
    data = response.get("payload", {}).get("data") or {}
    if not isinstance(data, dict):
        return None, None
    movement = (
        data
        .get("AAM", {})
        .get("M", {})
    )
    if not isinstance(movement, dict):
        return None, None
    mid = movement.get("MID")
    tt = movement.get("TT")
    try:
        mid = int(mid)
    except (TypeError, ValueError):
        mid = None
    try:
        tt = int(tt)
    except (TypeError, ValueError):
        tt = None
    return mid, tt


def process_async_events(events: RuntimeEvents, active: list[ActiveAttack]) -> list[ActiveAttack]:
    while True:
        try:
            event = events.messages.get_nowait()
        except queue.Empty:
            break
        payload = event.get("payload", {})
        command = payload.get("command")
        data = payload.get("data") or {}
        if command == "dms":
            mids = set(data.get("MID") or [])
            if mids:
                active = [attack for attack in active if attack.march_id not in mids]
                log(f"dms completed mids={sorted(mids)} active={len(active)}")
        elif command == "cat":
            mid, tt = attack_response_movement(event)
            if mid is not None:
                log(f"cat march_id={mid} travel_seconds={tt}")
        elif command == "cra":
            mid, tt = attack_response_movement(event)
            if mid is not None:
                log(f"cra async march_id={mid} travel_seconds={tt}")
    return active


def send_attack(
    socket,
    payload: dict[str, Any],
    *,
    timeout: float,
) -> dict:
    from event_worker.attack_send import raise_for_attack_status

    socket.send_json_command("cra", payload)
    response = socket.wait_for_json_response("cra", timeout=timeout)
    raise_for_attack_status(response)
    return response


class attack:
    """One active attack slot.

    Kept with the lowercase name from the original sketch so the file still
    maps directly to the outlined idea.
    """

    def __init__(
        self,
        time_sent: float,
        x: int,
        y: int,
        commander_id: int,
        *,
        release_at: float,
        march_id: int | None = None,
        travel_seconds: int | None = None,
    ) -> None:
        self.time_sent = time_sent
        self.x = x
        self.y = y
        self.commander_id = commander_id
        self.release_at = release_at
        self.march_id = march_id
        self.travel_seconds = travel_seconds
        self.outcome = False

    @classmethod
    def from_active(cls, item: ActiveAttack) -> "attack":
        return cls(
            item.sent_at,
            item.target_x,
            item.target_y,
            item.lord_id,
            release_at=item.release_at,
            march_id=item.march_id,
            travel_seconds=item.travel_seconds,
        )

    def to_active(self) -> ActiveAttack:
        return ActiveAttack(
            lord_id=self.commander_id,
            target_x=self.x,
            target_y=self.y,
            sent_at=self.time_sent,
            release_at=self.release_at,
            march_id=self.march_id,
            travel_seconds=self.travel_seconds,
        )


class berimond_attack_manager:
    """Manager following the original open/find/attack/start outline."""

    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.ATTACK_COOLDOWN_INTERVAL = args.attack_gap_min
        self.COMMANDER_COUNT = args.max_active
        self.timeout_until = time.time() - 5
        self.active_attacks: list[attack] = []
        self.last_attacked = 0.0
        self.return_speed_multiplier = args.return_multiplier
        self.events = RuntimeEvents(log_messages=not args.quiet_messages)
        self.rejected: set[tuple[int, int]] = set()
        self.successes = 0
        self.consecutive_failures = 0
        self.socket = None
        self.capture: dict[str, Any] = {}
        self.source_x = 0
        self.source_y = 0
        self.source_castle_id = -1
        self.runtime_config: tempfile.NamedTemporaryFile | None = None

    def active_as_core(self) -> list[ActiveAttack]:
        return [item.to_active() for item in self.active_attacks]

    def set_active_from_core(self, active: list[ActiveAttack]) -> None:
        self.active_attacks = [attack.from_active(item) for item in active]

    def save_state(self) -> None:
        save_json(
            self.args.state,
            {"active": [item.to_active().__dict__ for item in self.active_attacks]},
        )

    def open_berimond_ui(self) -> None:
        # Original sketch:
        #   %xt%EmpireEx_19%klh%1%{}%
        #   %xt%EmpireEx_19%pep%1%{"EID":3}%
        open_berimond(self.socket, self.args)

    def find_closest_watchtower(self) -> tuple[int, int, int]:
        captured_target = (int(self.capture["TX"]), int(self.capture["TY"]))
        if captured_target not in self.rejected:
            log(f"using_captured_target target=({captured_target[0]},{captured_target[1]})")
            return captured_target[0], captured_target[1], -1

        log(
            f"scan_fallback_start source=({self.source_x},{self.source_y}) "
            f"radius={self.args.scan_radius} target_type={self.args.target_type}"
        )
        return find_target(
            self.socket,
            sx=self.source_x,
            sy=self.source_y,
            kingdom=BERIMOND,
            npc_type=self.args.target_type,
            scan_radius=self.args.scan_radius,
            scan_hint_dx=self.args.scan_hint_dx,
            scan_hint_dy=self.args.scan_hint_dy,
            scan_delay_min=self.args.scan_delay_min,
            scan_delay_max=self.args.scan_delay_max,
            scan_delay_jitter=self.args.scan_delay_jitter,
            rejected=self.rejected,
        )

    def attack(self, x: int, y: int, level: int = -1) -> attack:
        # Original capture is stored verbatim in berimond_attack_capture.json;
        # only source, target, kingdom and commander are changed per send.
        target_info = self.socket.get_target_infos(
            BERIMOND, self.source_x, self.source_y, x, y, quiet=True
        )
        if isinstance(target_info, dict):
            lord_id = resolve_lord(self.socket, target_info, self.active_as_core())
        else:
            from event_worker.lords import resolve_lord_id

            lord_id = resolve_lord_id(self.socket.get_lords())
            excluded = {item.commander_id for item in self.active_attacks}
            if lord_id in excluded:
                rows = self.socket.get_lords().get("payload", {}).get("data", {}).get("C", [])
                for row in rows:
                    candidate = row.get("ID") if isinstance(row, dict) else row[0]
                    if int(candidate) not in excluded:
                        lord_id = int(candidate)
                        break
        payload = build_payload(
            self.capture,
            sx=self.source_x,
            sy=self.source_y,
            tx=x,
            ty=y,
            lord_id=lord_id,
        )

        if self.args.dry_run:
            log(f"dry_run target=({x},{y}) level={level} lord={lord_id}")
            return attack(time.time(), x, y, lord_id, release_at=time.time())

        gap = random.uniform(self.args.attack_gap_min, self.args.attack_gap_max)
        log(f"pre_attack_wait={gap:.1f}s target=({x},{y}) lord={lord_id}")
        time.sleep(gap)

        try:
            response = send_attack(
                self.socket,
                payload,
                timeout=self.args.response_timeout,
            )
        except Exception as exc:
            raise AttackAttemptError(
                str(exc),
                lord_id=lord_id,
                target=(x, y),
            ) from exc
        mid, tt = attack_response_movement(response)
        sent_at = time.time()
        travel_seconds = tt if tt is not None else 120
        release_at = sent_at + travel_seconds * (1.0 + self.return_speed_multiplier) + 5
        item = attack(
            sent_at,
            x,
            y,
            lord_id,
            release_at=release_at,
            march_id=mid,
            travel_seconds=tt,
        )
        self.active_attacks.append(item)
        self.last_attacked = sent_at
        self.successes += 1
        self.consecutive_failures = 0
        log(
            f"attack_sent status={response.get('payload', {}).get('status')} "
            f"target=({x},{y}) level={level} lord={lord_id} "
            f"march_id={mid} travel_seconds={tt} "
            f"active={len(self.active_attacks)}/{self.COMMANDER_COUNT}"
        )
        return item

    def prune_active_attacks(self) -> None:
        before = self.active_as_core()
        active = process_async_events(self.events, before)
        active = prune_active(active, self.socket)
        before_keys = {(item.lord_id, item.march_id) for item in before}
        after_keys = {(item.lord_id, item.march_id) for item in active}
        released = before_keys - after_keys
        for lord_id, march_id in sorted(released):
            log(f"commander_released lord={lord_id} march_id={march_id}")
        self.set_active_from_core(active)

    def handle_failed_attack(self, exc: Exception) -> bool:
        self.consecutive_failures += 1
        log(
            f"attack_error consecutive={self.consecutive_failures} error={exc}",
            error=True,
        )
        if isinstance(exc, AttackAttemptError) and exc.lord_id is not None:
            target_x, target_y = exc.target or (int(self.capture["TX"]), int(self.capture["TY"]))
            release_at = time.time() + self.args.failure_slot_penalty
            self.active_attacks.append(
                attack(
                    time.time(),
                    target_x,
                    target_y,
                    exc.lord_id,
                    release_at=release_at,
                )
            )
            log(
                f"failure_slot_hold lord={exc.lord_id} "
                f"target=({target_x},{target_y}) "
                f"release_in={self.args.failure_slot_penalty:.1f}s",
                error=True,
            )
            self.save_state()
        try:
            self.socket.go_to_castle(GREEN, -1, quiet=True)
        except Exception:
            pass
        if self.consecutive_failures >= self.args.max_consecutive_failures:
            log("stopping after repeated attack failures", error=True)
            return False
        log(f"failure_process_wait={self.args.failure_process_timeout:.1f}s", error=True)
        time.sleep(self.args.failure_process_timeout)
        self.open_berimond_ui()
        return True

    def start(self) -> int:
        from event_worker.gge_session import connect_and_login, disconnect

        self.capture = load_capture(self.args.capture.expanduser().resolve())
        if self.args.account_config is not None:
            config_path = self.args.account_config.expanduser().resolve()
            log(f"using account_config={config_path}")
        else:
            self.runtime_config = write_runtime_config(
                server_url=self.args.server_url,
                login_details=self.args.login_details.expanduser().resolve(),
                login_capture=self.args.login_capture.expanduser().resolve(),
            )
            config_path = Path(self.runtime_config.name)
        stored = [
            ActiveAttack(**item)
            for item in load_json(self.args.state, {}).get("active", [])
            if isinstance(item, dict)
        ]
        self.set_active_from_core(stored)

        self.socket = connect_and_login(
            config_path=config_path,
            humanize=False,
            on_message=self.events.on_message,
        )
        interrupted = False
        try:
            random_delay(
                "post_login_settle",
                minimum=self.args.post_login_settle,
                maximum=self.args.post_login_settle,
                jitter=self.args.step_jitter,
            )
            self.open_berimond_ui()
            random_delay(
                "pre_source_lookup",
                minimum=self.args.step_delay_min,
                maximum=self.args.step_delay_max,
                jitter=self.args.step_jitter,
            )
            self.source_x, self.source_y, self.source_castle_id = choose_source(
                self.socket, self.capture
            )
            log(
                f"starting source=({self.source_x},{self.source_y}) "
                f"castle_id={self.source_castle_id} "
                f"max_active={self.COMMANDER_COUNT} dry_run={self.args.dry_run}"
            )

            while self.args.max_successes <= 0 or self.successes < self.args.max_successes:
                self.prune_active_attacks()
                self.save_state()

                if len(self.active_attacks) >= self.COMMANDER_COUNT:
                    wait = max(
                        5.0,
                        min(item.release_at for item in self.active_attacks)
                        - time.time(),
                    )
                    log(
                        f"commander_limit active={len(self.active_attacks)}/"
                        f"{self.COMMANDER_COUNT} wait={wait:.1f}s"
                    )
                    time.sleep(min(wait, 30.0))
                    continue

                try:
                    x, y, level = self.find_closest_watchtower()
                    try:
                        self.attack(x, y, level)
                    except Exception:
                        self.rejected.add((x, y))
                        raise
                    if self.args.dry_run:
                        return 0
                except Exception as exc:
                    if not self.handle_failed_attack(exc):
                        return 2
        except KeyboardInterrupt:
            interrupted = True
            log("received Ctrl+C; saving state and disconnecting", error=True)
        finally:
            try:
                self.save_state()
            finally:
                if self.socket is not None:
                    disconnect(self.socket)
                if self.runtime_config is not None:
                    try:
                        os.unlink(self.runtime_config.name)
                    except OSError:
                        pass
        return 130 if interrupted else 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Repeat Berimond captured attacks.")
    parser.add_argument("--scan-root", type=Path, default=DEFAULT_SCAN_ROOT)
    parser.add_argument("--server-url", default=DEFAULT_SERVER_URL)
    parser.add_argument(
        "--account-config",
        type=Path,
        default=DEFAULT_ACCOUNT_CONFIG,
        help="Use an existing account .ini directly, e.g. ~/Desktop/scan_coordinates/ventrilo.ini.",
    )
    parser.add_argument("--login-details", type=Path, default=DEFAULT_LOGIN_DETAILS)
    parser.add_argument("--login-capture", type=Path, default=DEFAULT_LOGIN_CAPTURE)
    parser.add_argument("--capture", type=Path, default=DEFAULT_CAPTURE)
    parser.add_argument("--state", type=Path, default=DEFAULT_STATE)
    parser.add_argument("--max-active", type=int, default=4)
    parser.add_argument("--max-successes", type=int, default=0)
    parser.add_argument("--target-type", type=int, default=2)
    parser.add_argument("--scan-radius", type=int, default=350)
    parser.add_argument("--scan-hint-dx", type=int, default=200)
    parser.add_argument("--scan-hint-dy", type=int, default=0)
    parser.add_argument("--scan-delay-min", type=float, default=1.4)
    parser.add_argument("--scan-delay-max", type=float, default=2.6)
    parser.add_argument("--scan-delay-jitter", type=float, default=0.5)
    parser.add_argument("--response-timeout", type=float, default=12.0)
    parser.add_argument("--attack-gap-min", type=float, default=12.0)
    parser.add_argument("--attack-gap-max", type=float, default=20.0)
    parser.add_argument("--failure-slot-penalty", type=float, default=20.0)
    parser.add_argument("--failure-process-timeout", type=float, default=20.0)
    parser.add_argument("--max-consecutive-failures", type=int, default=4)
    parser.add_argument("--return-multiplier", type=float, default=0.2)
    parser.add_argument("--post-login-settle", type=float, default=4.0)
    parser.add_argument("--step-delay-min", type=float, default=2.0)
    parser.add_argument("--step-delay-max", type=float, default=3.0)
    parser.add_argument("--step-jitter", type=float, default=0.7)
    parser.add_argument(
        "--quiet-messages",
        action="store_true",
        help="Do not print one-line summaries for incoming websocket messages.",
    )
    parser.add_argument("--dry-run", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.max_active < 1 or args.max_active > 4:
        raise ValueError("--max-active must be between 1 and 4 for this runner")
    if args.attack_gap_min < 5 or args.attack_gap_max < args.attack_gap_min:
        raise ValueError("attack gap must be at least 5s and max >= min")

    add_scan_paths(args.scan_root.expanduser().resolve())
    return berimond_attack_manager(args).start()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        log("received Ctrl+C during startup; exiting", error=True)
        raise SystemExit(130)
