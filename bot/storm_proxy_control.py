from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path
from typing import Any


if __package__ in {None, ""}:
    REPO_ROOT = Path(__file__).resolve().parents[1]
    if str(REPO_ROOT) not in sys.path:
        sys.path.insert(0, str(REPO_ROOT))
else:
    REPO_ROOT = Path(__file__).resolve().parents[1]

from empire.bot.storm_database import cleanup_expired_storm_targets, ensure_storm_tables
from empire.bot.test_psql_connection import connect, read_connection_config


CONTROL_FILE = REPO_ROOT / "empire" / "bot" / "proxy_control.json"
DEFAULT_LEVELS = (60, 70, 80)
DEFAULT_SOURCE_X = 675
DEFAULT_SOURCE_Y = 675
DEFAULT_SCAN_RADIUS = 52
DEFAULT_SCAN_MIN_SECONDS = 4.0
DEFAULT_SCAN_MAX_SECONDS = 6.0
DEFAULT_TARGET_FRESH_SECONDS = 120


def load_control() -> dict[str, Any]:
    if not CONTROL_FILE.exists():
        return {}
    try:
        data = json.loads(CONTROL_FILE.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {}
    return data if isinstance(data, dict) else {}


def save_control(state: dict[str, Any]) -> None:
    CONTROL_FILE.parent.mkdir(parents=True, exist_ok=True)
    tmp = CONTROL_FILE.with_suffix(CONTROL_FILE.suffix + ".tmp")
    tmp.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    tmp.replace(CONTROL_FILE)


def parse_levels(value: str | None) -> list[int]:
    if not value:
        return list(DEFAULT_LEVELS)
    levels = []
    for item in value.split(","):
        item = item.strip()
        if not item:
            continue
        levels.append(int(item))
    return sorted(set(levels))


def ensure_ready() -> None:
    with connect(read_connection_config()) as conn:
        ensure_storm_tables(conn)
        cleanup_expired_storm_targets(conn)


def start(args: argparse.Namespace) -> None:
    ensure_ready()
    state = load_control()
    state.update(
        {
            "running": True,
            "transport_only": True,
            "mode": "storm",
            "pending": None,
            "last_cra": None,
            "attacks_sent": 0,
            "max_attacks": int(args.max_attacks or 0),
            "storm_task": args.task,
            "target_levels": parse_levels(args.levels),
            "storm_source": {
                "x": int(args.source_x),
                "y": int(args.source_y),
                "hbw": int(args.hbw),
            },
            "storm_ptt": int(args.ptt),
            "storm_scan": {
                "enabled": True,
                "center_x": int(args.source_x),
                "center_y": int(args.source_y),
                "radius": int(args.scan_radius),
                "min_seconds": float(args.scan_min),
                "max_seconds": float(args.scan_max),
                "next_at": 0.0,
                "cursor": 0,
                "chunks": [],
            },
            "target_fresh_seconds": int(args.target_fresh_seconds),
            "last_gaa_scan": None,
            "runner": "bot.py",
            "started_at": int(time.time()),
            "stop_reason": None,
        }
    )
    save_control(state)
    print(
        "storm_start "
        f"task={state['storm_task']} source={args.source_x}:{args.source_y} "
        f"levels={state['target_levels']} max_attacks={state['max_attacks']} "
        f"scan={args.scan_min:.1f}-{args.scan_max:.1f}s radius={args.scan_radius} "
        "transport_only=true run_proxy_botv2_to_drive"
    )


def end(_: argparse.Namespace) -> None:
    state = load_control()
    state["running"] = False
    state["pending"] = None
    state["stopped_at"] = int(time.time())
    state["stop_reason"] = "manual_end"
    save_control(state)
    print("storm_end running=false")


def status(_: argparse.Namespace) -> None:
    ensure_ready()
    state = load_control()
    print(
        "control "
        f"running={state.get('running')} mode={state.get('mode')} "
        f"pending={state.get('pending')} attacks_sent={state.get('attacks_sent')} "
        f"max_attacks={state.get('max_attacks')}"
    )
    scan = state.get("storm_scan")
    if isinstance(scan, dict):
        print(
            "storm_scan "
            f"enabled={scan.get('enabled')} center={scan.get('center_x')}:{scan.get('center_y')} "
            f"radius={scan.get('radius')} interval={scan.get('min_seconds')}-{scan.get('max_seconds')} "
            f"cursor={scan.get('cursor')}"
        )
    with connect(read_connection_config()) as conn:
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT target_level, status, count(*)
                FROM storm_target
                WHERE kingdom_id = 4
                  AND area_type = 25
                  AND expires_at > extract(epoch from now())::bigint
                GROUP BY target_level, status
                ORDER BY target_level NULLS FIRST, status
                """
            )
            for level, target_status, count in cur.fetchall():
                print(f"storm_targets level={level} status={target_status} count={count}")
            cur.execute(
                """
                SELECT march_id, x_coordinate, y_coordinate, target_level, status, result_flag
                FROM attack
                WHERE target_kind = 'storm_target'
                ORDER BY time_created DESC NULLS LAST
                LIMIT 10
                """
            )
            for row in cur.fetchall():
                print(
                    "storm_attack "
                    f"mid={row[0]} target={row[1]}:{row[2]} level={row[3]} "
                    f"status={row[4]} result_flag={row[5]}"
                )


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Control the active mitmproxy Storm runner.")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--start", action="store_true")
    group.add_argument("--end", action="store_true")
    group.add_argument("--status", action="store_true")
    parser.add_argument("--source-x", type=int, default=DEFAULT_SOURCE_X)
    parser.add_argument("--source-y", type=int, default=DEFAULT_SOURCE_Y)
    parser.add_argument("--hbw", type=int, default=-1)
    parser.add_argument("--ptt", type=int, default=1)
    parser.add_argument("--task", default="storm_custom")
    parser.add_argument("--levels", default="60,70,80")
    parser.add_argument("--max-attacks", type=int, default=1)
    parser.add_argument("--scan-min", type=float, default=DEFAULT_SCAN_MIN_SECONDS)
    parser.add_argument("--scan-max", type=float, default=DEFAULT_SCAN_MAX_SECONDS)
    parser.add_argument("--scan-radius", type=int, default=DEFAULT_SCAN_RADIUS)
    parser.add_argument("--target-fresh-seconds", type=int, default=DEFAULT_TARGET_FRESH_SECONDS)
    args = parser.parse_args(argv)
    if args.scan_max < args.scan_min:
        parser.error("--scan-max must be >= --scan-min")
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.start:
        start(args)
    elif args.end:
        end(args)
    else:
        status(args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
