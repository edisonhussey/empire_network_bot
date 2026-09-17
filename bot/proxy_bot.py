from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime
from pathlib import Path
from types import SimpleNamespace


def find_repo_root(path: Path) -> Path:
    for parent in (path, *path.parents):
        if (parent / "bot" / "scheduler.py").is_file():
            return parent
    return path.parents[1]


REPO_ROOT = find_repo_root(Path(__file__).resolve())
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from bot import bot as core
from bot import accounts
from bot.storm_database import cleanup_expired_storm_targets, ensure_storm_tables
from bot.test_psql_connection import connect, read_connection_config
from bot import sands_proxy
from bot import scheduler as task_scheduler


CONTROL_FILE = REPO_ROOT / "bot" / "proxy_control.json"
DEFAULT_LOG_DIR = REPO_ROOT / "bot" / "logs"


def configure_account(account_name: str | None, aid: str | None = None):
    global CONTROL_FILE, DEFAULT_LOG_DIR
    context = core.configure_account(account_name, aid)
    CONTROL_FILE = context.control_file
    DEFAULT_LOG_DIR = context.logs_dir
    return context


def load_control() -> dict:
    if not CONTROL_FILE.exists():
        return {}
    try:
        data = json.loads(CONTROL_FILE.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {}
    return data if isinstance(data, dict) else {}


def status() -> None:
    state = load_control()
    print(
        "control "
        f"running={state.get('running')} transport_only={state.get('transport_only')} "
        f"mode={state.get('mode')} pending={state.get('pending')} "
        f"attacks_sent={state.get('attacks_sent')} max_attacks={state.get('max_attacks')}"
    )
    with connect(read_connection_config()) as conn:
        core.ensure_bot_tables(conn)
        ensure_storm_tables(conn)
        cleanup_expired_storm_targets(conn)
        db_pending = core.load_proxy_pending_db(conn)
        db_last_cra = core.load_proxy_last_cra_db(conn)
        print(f"db_proxy_pending={db_pending}")
        print(f"db_proxy_last_cra={db_last_cra}")
        stats = core.attack_run_stats(conn, started_at=int(state.get("started_at") or 0))
        print(
            "db_run_stats "
            f"attacks={stats['db_attacks']} completed={stats['db_completed']} "
            f"coin_loot={stats['coin_loot']} ruby_loot={stats['ruby_loot']}"
        )
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT target_level, status, count(*)
                FROM storm_target
                WHERE aid = %s
                  AND kingdom_id = 4
                  AND area_type = 25
                  AND expires_at > extract(epoch from now())::bigint
                GROUP BY target_level, status
                ORDER BY target_level NULLS FIRST, status
                """,
                (core.current_aid(),),
            )
            for level, target_status, count in cur.fetchall():
                print(f"storm_targets level={level} status={target_status} count={count}")
            cur.execute(
                """
                SELECT march_id, x_coordinate, y_coordinate, target_level, lord_id, commander_number, status, duration
                FROM attack
                WHERE aid = %s
                  AND target_kind = 'storm_target'
                ORDER BY time_created DESC NULLS LAST
                LIMIT 10
                """,
                (core.current_aid(),),
            )
            for march_id, x, y, level, lid, commander_number, attack_status, duration in cur.fetchall():
                print(
                    "storm_attack "
                    f"mid={march_id} target={x}:{y} level={level} lid={lid} commander={commander_number} "
                    f"status={attack_status} travel_duration={duration}"
                )


def build_core_args(args: argparse.Namespace) -> SimpleNamespace:
    max_cra_per_hour = int(args.max_cra_per_hour)
    if max_cra_per_hour <= 0:
        max_cra_per_hour = int(args.max_attacks)
    return SimpleNamespace(
        max_attacks=int(args.max_attacks),
        max_cra_per_hour=max_cra_per_hour,
        storm_task=selected_storm_task_name(args),
        levels=args.levels,
        source_x=int(args.source_x),
        source_y=int(args.source_y),
        hbw=int(args.hbw),
        ptt=int(args.ptt),
        scan_min=float(args.scan_min),
        scan_max=float(args.scan_max),
        scan_radius=int(args.scan_radius),
        target_fresh_seconds=int(args.target_fresh_seconds),
        use_randomizer_gaa_wait=bool(args.use_randomizer_gaa_wait),
    )


def scheduler_task(task_name: str):
    if not task_name:
        return None
    for task in task_scheduler.TASKS:
        if task.name == task_name:
            return task
    return None


def selected_storm_task_name(args: argparse.Namespace) -> str:
    if args.task:
        return args.task
    for task in task_scheduler.TASKS:
        if task.enabled and int(task.kingdom_id) != int(core.SANDS_KID):
            return task.name
    return "storm_custom"


def should_run_sands(args: argparse.Namespace) -> bool:
    if args.mode == "sands":
        return True
    if args.mode == "storm":
        return False
    enabled = [task for task in task_scheduler.TASKS if task.enabled]
    if enabled:
        return all(int(task.kingdom_id) == int(core.SANDS_KID) for task in enabled)
    task = scheduler_task(args.task)
    if task is not None:
        return int(task.kingdom_id) == int(core.SANDS_KID)
    return False


def start(args: argparse.Namespace) -> int:
    account = accounts.load_account(args.account_name)
    mismatch = accounts.session_mismatch(account)
    if mismatch:
        print(f"REFUSING TO START: {mismatch}", file=sys.stderr)
        print(
            "Log in with that account, or pass the matching --<username>. "
            "Check with: python -m bot.cli session",
            file=sys.stderr,
        )
        return 2

    if should_run_sands(args):
        return sands_proxy.main(["--account-name", args.account_name, "--start", "--max-attacks", str(int(args.max_attacks))])

    args.log_dir.mkdir(parents=True, exist_ok=True)
    core.LOG_FILE = (args.log_dir / datetime.now().strftime("proxy_bot_%Y%m%d_%H%M%S.log")).open(
        "a",
        encoding="utf-8",
    )
    try:
        return core.run_storm_proxy(build_core_args(args))
    except KeyboardInterrupt:
        core.stop_proxy_transport("keyboard_interrupt")
        print("\nproxy_bot stopped from Ctrl+C", flush=True)
        return 130
    finally:
        if core.LOG_FILE is not None:
            core.LOG_FILE.close()
            core.LOG_FILE = None


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Proxy driver. Sands uses the old ADI/CRA DB loop; Storm drives scans.")
    accounts.add_account_arguments(parser)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--start", action="store_true", help="drive attacks through the active mitmproxy session")
    group.add_argument("--end", action="store_true", help="stop the driver and clear pending proxy work")
    group.add_argument("--status", action="store_true", help="show current control/database state")
    parser.add_argument("--mode", choices=("auto", "sands", "storm"), default="auto")
    parser.add_argument("--max-attacks", type=int, default=1)
    parser.add_argument("--max-cra-per-hour", type=int, default=0, help="0 means use --max-attacks for this run")
    parser.add_argument("--task", default=None)
    parser.add_argument("--levels", default="60,70,80")
    parser.add_argument("--source-x", type=int, default=core.STORM_SOURCE_X)
    parser.add_argument("--source-y", type=int, default=core.STORM_SOURCE_Y)
    parser.add_argument("--hbw", type=int, default=core.STORM_HBW)
    parser.add_argument("--ptt", type=int, default=core.STORM_PTT)
    parser.add_argument("--scan-min", type=float, default=core.STORM_SCAN_INTERVAL_RANGE[0])
    parser.add_argument("--scan-max", type=float, default=core.STORM_SCAN_INTERVAL_RANGE[1])
    parser.add_argument("--scan-radius", type=int, default=core.STORM_SCAN_RADIUS)
    parser.add_argument("--target-fresh-seconds", type=int, default=core.STORM_TARGET_FRESH_SECONDS)
    parser.add_argument("--use-randomizer-gaa-wait", action="store_true")
    parser.add_argument("--log-dir", type=Path, default=None)
    args = parser.parse_args(accounts.apply_account_flags(list(sys.argv[1:] if argv is None else argv)))
    if args.scan_max < args.scan_min:
        parser.error("--scan-max must be >= --scan-min")
    if args.start and args.max_attacks < 1:
        parser.error("--max-attacks must be >= 1")
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    account = accounts.account_from_args(args)
    args.account_name = account.name
    context = configure_account(account.username or None, account.aid)
    if args.log_dir is None:
        args.log_dir = context.logs_dir
    if args.status:
        status()
        return 0
    if args.end:
        state = load_control()
        if state.get("mode") == "sands":
            return sands_proxy.main(["--account-name", args.account_name, "--end"])
        core.stop_proxy_transport("manual_end")
        print("proxy_bot stopped; mitmproxy session can stay open", flush=True)
        return 0
    return start(args)


if __name__ == "__main__":
    raise SystemExit(main())
