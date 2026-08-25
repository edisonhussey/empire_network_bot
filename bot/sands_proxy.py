from __future__ import annotations

import argparse
import json
import re
import sys
import time
from pathlib import Path


def find_repo_root(path: Path) -> Path:
    for parent in (path, *path.parents):
        if (parent / "bot" / "scheduler.py").is_file():
            return parent
    return path.parents[2]


REPO_ROOT = find_repo_root(Path(__file__).resolve())
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from bot import bot
from bot.test_psql_connection import connect, read_connection_config


CONTROL_FILE = REPO_ROOT / "bot" / "proxy_control.json"
CONTROL_LOG = REPO_ROOT / "bot" / "rbc_proxy_listener.log"
ACK_GRACE_SECONDS = 20.0

IMPORTANT_LOG_PATTERNS = (
    "rbc_proxy_listener_loaded",
    "proxy_idle",
    "proxy_adi_sent",
    "proxy_adi_skip",
    "proxy_cra_pending",
    "proxy_cra_sent",
    "proxy_cra_ack",
    "proxy_cra_error",
    "proxy_cra_ack_unparsed",
    "proxy_cat_return",
    "proxy_loop_error",
    "proxy_stopped",
)


def configure_account(account_name: str):
    global CONTROL_FILE, CONTROL_LOG
    context = bot.configure_account(account_name)
    CONTROL_FILE = context.control_file
    CONTROL_LOG = context.listener_log
    return context


def load_control() -> dict:
    if not CONTROL_FILE.exists():
        return {}
    try:
        data = json.loads(CONTROL_FILE.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {}
    return data if isinstance(data, dict) else {}


def save_control(data: dict) -> None:
    CONTROL_FILE.parent.mkdir(parents=True, exist_ok=True)
    tmp = CONTROL_FILE.with_suffix(CONTROL_FILE.suffix + ".tmp")
    tmp.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    tmp.replace(CONTROL_FILE)


def bot_duration_text(state: dict) -> str:
    try:
        started_at = int(state.get("started_at") or 0)
    except (TypeError, ValueError):
        started_at = 0
    if started_at <= 0:
        return "00:00"
    try:
        stopped_at = int(state.get("stopped_at") or 0)
    except (TypeError, ValueError):
        stopped_at = 0
    end_at = stopped_at if stopped_at > 0 else bot.now_epoch()
    seconds = max(0, end_at - started_at)
    hours, remainder = divmod(seconds, 3600)
    minutes = remainder // 60
    return f"{hours:02d}:{minutes:02d}"


def stop_summary(state: dict, reason: str | None = None) -> str:
    reason_text = reason or state.get("stop_reason") or "stopped"
    attacks_sent = int(state.get("attacks_sent", 0) or 0)
    stats_text = ""
    try:
        with connect(read_connection_config()) as conn:
            stats = bot.attack_run_stats(conn, started_at=int(state.get("started_at") or 0))
        stats_text = (
            f" db_attacks={stats['db_attacks']} db_completed={stats['db_completed']} "
            f"coin_loot={stats['coin_loot']} ruby_loot={stats['ruby_loot']}"
        )
    except Exception:
        stats_text = ""
    return f"reason={reason_text} attacks_sent={attacks_sent} duration={bot_duration_text(state)}{stats_text}"


def stop_control(state: dict, reason: str) -> dict:
    state["running"] = False
    state["pending"] = None
    state["stopped_at"] = bot.now_epoch()
    state["stop_reason"] = reason
    save_control(state)
    return state


def important_log_line(line: str) -> bool:
    if "websocket_start" in line:
        return "selected=yes" in line
    if "rbc_upsert" in line and "source=adi" in line:
        return True
    return any(pattern in line for pattern in IMPORTANT_LOG_PATTERNS)


def terminal_line(line: str) -> str:
    stripped = line.rstrip()
    match = re.match(r"^\[(?P<ts>[^\]]+)\]\s+(?P<msg>.*)$", stripped)
    if not match:
        return stripped
    timestamp = match.group("ts").split(" ")[-1]
    return f"[{timestamp}] {match.group('msg')}"


def print_new_log_lines(start_pos: int) -> int:
    if not CONTROL_LOG.exists():
        return start_pos
    with CONTROL_LOG.open("r", encoding="utf-8", errors="replace") as handle:
        handle.seek(start_pos)
        for line in handle:
            if important_log_line(line):
                print(terminal_line(line), flush=True)
        return handle.tell()


def monitor_start() -> None:
    position = CONTROL_LOG.stat().st_size if CONTROL_LOG.exists() else 0
    stopped_seen_at = None
    print("proxy bot monitor active; Ctrl+C stops the control loop", flush=True)
    while True:
        position = print_new_log_lines(position)
        state = load_control()
        if not state.get("running"):
            if isinstance(state.get("last_cra"), dict):
                if stopped_seen_at is None:
                    stopped_seen_at = time.time()
                if time.time() - stopped_seen_at < ACK_GRACE_SECONDS:
                    time.sleep(0.5)
                    continue
            reason = state.get("stop_reason") or "stopped"
            print(f"proxy bot stopped {stop_summary(state, reason)}", flush=True)
            return
        stopped_seen_at = None
        time.sleep(0.5)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Control the mitmproxy Sands level-61 RBC process.")
    parser.add_argument("--account-name", "--username", dest="account_name", required=True)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--start", action="store_true", help="start the proxy-controlled bot loop")
    group.add_argument("--end", action="store_true", help="stop the bot loop while leaving mitmproxy logged in")
    group.add_argument("--status", action="store_true", help="print current proxy control state")
    parser.add_argument("--max-attacks", type=int, default=1, help="stop after this many CRA packets are sent")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    context = configure_account(args.account_name)
    state = load_control()
    if args.status:
        print(json.dumps(state, indent=2, sort_keys=True))
        return 0
    if args.end:
        state = stop_control(state, "manual_end")
        print(f"proxy bot stopped {stop_summary(state)}; mitmproxy session can stay open ({CONTROL_FILE})")
        return 0

    state = {
        "running": True,
        "mode": "sands",
        "transport_only": False,
        "runner": "proxy_bot.py",
        "username": context.username,
        "aid": context.aid,
        "account_root": str(context.root),
        "control_file": str(CONTROL_FILE),
        "max_attacks": max(0, int(args.max_attacks)),
        "attacks_sent": 0,
        "started_at": bot.now_epoch(),
        "stopped_at": None,
        "pending": None,
        "last_cra": None,
        "cra_consecutive_errors": 0,
        "cra_error_timestamps": [],
        "stop_reason": None,
    }
    save_control(state)
    print(f"proxy bot started mode=sands max_attacks={state['max_attacks']} control={CONTROL_FILE}")
    try:
        monitor_start()
    except KeyboardInterrupt:
        state = stop_control(load_control(), "keyboard_interrupt")
        print(
            f"\nproxy bot stopped from Ctrl+C; {stop_summary(state)}",
            flush=True,
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
