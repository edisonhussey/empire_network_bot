from __future__ import annotations

import argparse
import json
import math
import random
import sys
import time
from pathlib import Path
from types import SimpleNamespace
from typing import Any


def find_repo_root(path: Path) -> Path:
    for parent in (path, *path.parents):
        if (parent / "empire").is_dir():
            return parent
    return path.parents[1]


REPO_ROOT = find_repo_root(Path(__file__).resolve())
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))
BOT_DIR = Path(__file__).resolve().parent
if str(BOT_DIR) not in sys.path:
    sys.path.insert(0, str(BOT_DIR))

from ranomizer import Randomizer
from send_gaa_probe import CONTROL_FILE, STORM_KID, latest_client_gaa, queue_probe, wait_for_send


MAP_CHUNK_SIZE = 13
DEFAULT_RADIUS = 52
MIN_GAA_SECONDS = 8.0


def load_json(path: Path, default: dict[str, Any]) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError):
        return default.copy()
    return data if isinstance(data, dict) else default.copy()


def chunk_start(value: int) -> int:
    return max(0, int(value) - (int(value) % MAP_CHUNK_SIZE))


def discover_center(kid: int) -> tuple[int, int]:
    latest = latest_client_gaa(kid)
    if latest is None:
        raise SystemExit("no recent Storm GAA chunk found; pass --center-x and --center-y")
    ax1, ay1, _header = latest
    return ax1 + MAP_CHUNK_SIZE // 2, ay1 + MAP_CHUNK_SIZE // 2


def uncertain_chunks(center_x: int, center_y: int, radius: int) -> list[tuple[int, int]]:
    offsets: list[tuple[int, int]] = []
    steps = range(-radius, radius + 1, MAP_CHUNK_SIZE)
    for dx in steps:
        for dy in steps:
            if math.hypot(dx, dy) <= radius + MAP_CHUNK_SIZE / 2:
                offsets.append((dx, dy))
    random.shuffle(offsets)
    offsets.sort(key=lambda item: math.hypot(item[0], item[1]) + random.uniform(-22.0, 22.0))
    return [(chunk_start(center_x + dx - 6), chunk_start(center_y + dy - 6)) for dx, dy in offsets]


def wait_for_pending_clear(timeout: float) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        state = load_json(CONTROL_FILE, {})
        if not state.get("pending"):
            return True
        time.sleep(0.75)
    return False


def sleep_scan_interval(randomizer: Randomizer, *, minimum: float) -> None:
    delay = max(float(minimum), float(randomizer.gaa_waiting_time()))
    print(f"storm_scan_wait seconds={delay:.1f}", flush=True)
    time.sleep(delay)


def queue_one(kid: int, ax1: int, ay1: int, timeout: float, force: bool) -> bool:
    args = SimpleNamespace(
        kid=kid,
        ax1=ax1,
        ay1=ay1,
        server_header=None,
        force=force,
    )
    queued = queue_probe(args)
    print(
        "storm_gaa_queued "
        f"kid={queued['kid']} chunk={queued['ax1']}:{queued['ay1']}-{queued['ax2']}:{queued['ay2']}",
        flush=True,
    )
    sent = wait_for_send(float(queued["queued_at"]), timeout)
    if sent is None:
        print("storm_gaa_send_timeout", flush=True)
        return False
    print(
        "storm_gaa_sent "
        f"kid={sent['kid']} chunk={sent['ax1']}:{sent['ay1']}-{sent['ax2']}:{sent['ay2']}",
        flush=True,
    )
    return True


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Queue randomized Storm GAA scan probes through mitmproxy.")
    parser.add_argument("--kid", type=int, default=STORM_KID)
    parser.add_argument("--center-x", type=int)
    parser.add_argument("--center-y", type=int)
    parser.add_argument("--radius", type=int, default=DEFAULT_RADIUS)
    parser.add_argument("--max-requests", type=int, default=0, help="0 means keep scanning until Ctrl+C")
    parser.add_argument("--min-wait", type=float, default=MIN_GAA_SECONDS)
    parser.add_argument("--send-timeout", type=float, default=20.0)
    parser.add_argument("--pending-timeout", type=float, default=45.0)
    parser.add_argument("--force", action="store_true", help="replace an existing pending probe")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    randomizer = Randomizer()
    if args.center_x is None or args.center_y is None:
        center_x, center_y = discover_center(args.kid)
    else:
        center_x, center_y = int(args.center_x), int(args.center_y)

    chunks = uncertain_chunks(center_x, center_y, int(args.radius))
    if not chunks:
        raise SystemExit("no chunks generated")

    print(
        f"storm_scan_start kid={args.kid} center={center_x}:{center_y} "
        f"radius={args.radius} chunks={len(chunks)} max_requests={args.max_requests}",
        flush=True,
    )

    sent_count = 0
    cursor = 0
    try:
        while args.max_requests <= 0 or sent_count < args.max_requests:
            if cursor >= len(chunks):
                cursor = 0
                chunks = uncertain_chunks(center_x, center_y, int(args.radius))
                print("storm_scan_new_pass", flush=True)

            ax1, ay1 = chunks[cursor]
            cursor += 1
            if not wait_for_pending_clear(float(args.pending_timeout)):
                print("storm_scan_wait_pending_timeout", flush=True)
                sleep_scan_interval(randomizer, minimum=max(args.min_wait, MIN_GAA_SECONDS))
                continue

            if queue_one(args.kid, ax1, ay1, float(args.send_timeout), bool(args.force)):
                sent_count += 1
            sleep_scan_interval(randomizer, minimum=max(args.min_wait, MIN_GAA_SECONDS))
    except KeyboardInterrupt:
        print(f"\nstorm_scan_stopped sent={sent_count}", flush=True)
        return 0
    print(f"storm_scan_done sent={sent_count}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
