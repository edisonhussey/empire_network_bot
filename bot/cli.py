"""Single entry point for every operator command.

    python -m bot.cli accounts
    python -m bot.cli attacks
    python -m bot.cli tasks
    python -m bot.cli troops

    python -m bot.cli db init
    python -m bot.cli db status
    python -m bot.cli --ventrilo db populate

    python -m bot.cli --ventrilo proxy start --max-attacks 200
    python -m bot.cli --ventrilo proxy status
    python -m bot.cli --ventrilo proxy end

Command groups that need an account accept either the ``--<username>`` shorthand
(one per ``credentials/*.env``) or the explicit ``--account-name``.

The lower-level modules (`bot.bot`, `bot.proxy_bot`, `bot.populate_database_rbc`)
remain runnable directly; this module just gives them one consistent surface and
keeps the per-account wiring in one place.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

import psycopg

if __package__ in {None, ""}:
    REPO_ROOT = Path(__file__).resolve().parents[1]
    if str(REPO_ROOT) not in sys.path:
        sys.path.insert(0, str(REPO_ROOT))

from bot import accounts as accounts_module
from bot import attacks as attacks_module
from bot import db as db_module
from bot import tasks as tasks_module


# ---------------------------------------------------------------------------
# Informational commands
# ---------------------------------------------------------------------------


def cmd_accounts(_args: argparse.Namespace) -> int:
    print(accounts_module.describe_accounts())
    return 0


def cmd_attacks(args: argparse.Namespace) -> int:
    for name, attack in sorted(attacks_module.ATTACK_REGISTRY.items()):
        waves = len(attack.to_payload())
        print(f"{name}: waves={waves}")
        if args.payload:
            print(json.dumps(attack.to_payload(), separators=(",", ":")))
    return 0


def cmd_tasks(args: argparse.Namespace) -> int:
    """Show the task plan, resolved for the account that will actually run."""

    if args.all:
        print("# DEFAULT plan")
        print(tasks_module.task_summary(tasks_module.tasks_for(None)))
        for name in accounts_module.available_accounts():
            print(f"\n# plan for {name}")
            print(tasks_module.task_summary(tasks_module.tasks_for(name)))
        return 0

    account = accounts_module.session_account()
    label = account.name if account is not None else "default (no login detected)"
    print(f"# plan for {label}")
    print(tasks_module.task_summary(tasks_module.tasks_for(account.name if account else None)))
    if args.plan:
        print(tasks_module.plan_summary())
    return 0


def cmd_session(_args: argparse.Namespace) -> int:
    """Show which account the listener last saw log in (identity = aid)."""

    session = accounts_module.read_session(max_age_seconds=0)
    if session is None:
        print("no login detected yet")
        print("start the listener (mitmdump -s bot/rbc_proxy_listener.py) and log in to the game")
        print("until then, pass an explicit account: " + ", ".join(
            f"--{name}" for name in accounts_module.available_accounts()
        ))
        return 1

    key = str(session.get("account") or session.get("aid") or "")
    username = str(session.get("username") or "")
    detected_at = int(session.get("detected_at") or 0)
    age = max(0, int(time.time()) - detected_at) if detected_at else -1
    print(
        f"detected_session account={username or key} key={key} "
        f"login_name={session.get('display_name') or 'unknown'} "
        f"age_seconds={age} source={session.get('source') or '?'}"
    )

    where = f"credentials/{username}.env" if username else "none (auto-created account)"
    print(f"  credentials {where}")
    print("  this is the account all commands use by default")

    try:
        with db_module.connection() as conn:
            counts = db_module.table_counts(conn, key)
    except psycopg.Error as exc:
        print(f"  db unavailable: {exc}")
        return 0

    missing = [name for name, count in counts.items() if count < 0]
    rbc_rows = counts.get("rbc", -1)
    print(
        f"  db rbc={rbc_rows if rbc_rows >= 0 else 'MISSING'} "
        f"commander_state={counts.get('commander_state', -1)}"
    )
    if missing:
        print(f"  missing tables: {', '.join(missing)} -> run: python -m bot.cli db init")
    elif rbc_rows == 0:
        print("  no RBC targets yet -> run: python -m bot.cli db populate")
    return 0


def cmd_troops(_args: argparse.Namespace) -> int:
    from bot.game_data import Tool, Troop

    for label, enum_type in (("troops", Troop), ("tools", Tool)):
        print(f"--- {label} ---")
        for member in enum_type:
            print(f"{getattr(member, 'id', '?')}\t{member.name}")
    return 0


# ---------------------------------------------------------------------------
# Database commands
# ---------------------------------------------------------------------------


def _print_counts(conn, aid: str, label: str) -> None:
    counts = db_module.table_counts(conn, aid)
    rendered = " ".join(f"{table}={count if count >= 0 else 'MISSING'}" for table, count in counts.items())
    print(f"db_counts aid={aid} ({label}) {rendered}")


def cmd_db_init(args: argparse.Namespace) -> int:
    aid = accounts_module.load_account(args.account).aid if args.account else None
    name = db_module.init_database(aid=aid)
    print(f"db_ready database={name} schema_applied=yes")
    with db_module.connection() as conn:
        db_module.ensure_schema(conn, aid)
        if aid:
            _print_counts(conn, aid, args.account)
    return 0


def cmd_db_schema(args: argparse.Namespace) -> int:
    aid = accounts_module.load_account(args.account).aid if args.account else None
    with db_module.connection() as conn:
        db_module.ensure_schema(conn, aid)
    print(f"schema_applied database={db_module.database_name()}")
    return 0


def cmd_db_logs(args: argparse.Namespace) -> int:
    """Show each capture folder's size, and optionally prune processed captures."""

    mib = 1024 * 1024
    cap = db_module.capture_max_bytes()
    print(f"capture_cap {cap // mib} MB per account (override with GGE_CAPTURE_MAX_MB)")

    bot_dir = accounts_module.account_context.BOT_DIR
    if args.folder:
        folders = [(Path(args.folder).name, Path(args.folder))]
    else:
        folders = [(d.parent.name, d) for d in sorted((bot_dir / "account_data").glob("*/logs"))]
        legacy = bot_dir / "logs"
        if legacy.is_dir():
            folders.append(("(legacy shared)", legacy))

    if not folders:
        print("no capture folders found")
        return 0

    for label, folder in folders:
        stats = db_module.capture_folder_stats(folder)
        mark = "OVER" if stats["bytes"] > cap else "ok"
        print(f"  {label:<18} files={stats['files']:<6} size_mb={stats['bytes'] // mib:<6} {mark}")

    if not args.prune:
        print("add --prune to delete already-processed captures above the cap")
        return 0

    if args.force and not args.folder:
        print("--force requires --folder, so it cannot sweep every account at once")
        return 1

    with db_module.connection() as conn:
        processed = db_module.all_processed_log_paths(conn)

    for label, folder in folders:
        result = db_module.prune_capture_logs(
            folder,
            max_bytes=cap,
            processed=processed,
            dry_run=not args.apply,
            require_processed=not args.force,
        )
        if not result["deleted"]:
            continue
        note = " (dry run)" if not args.apply else ""
        print(
            f"  {label}: delete {result['deleted']} files, free "
            f"{result['freed_bytes'] // mib}MB, kept {result['protected']} unprocessed{note}"
        )

    if not args.apply:
        print("dry run - re-run with --prune --apply to actually delete")
    return 0


def cmd_db_migrate_keys(args: argparse.Namespace) -> int:
    """Rename legacy numeric scope keys onto login-name keys."""

    mapping = accounts_module.legacy_key_map()
    if not mapping:
        print("nothing to migrate: no credentials file declares an AID different from its name")
        return 0

    print("planned renames:")
    for old, new in mapping.items():
        print(f"  {old} -> {new}")

    try:
        counts = db_module.migrate_account_keys(mapping, dry_run=not args.apply)
    except RuntimeError as exc:
        print(f"migration refused: {exc}")
        return 1

    for key, rows in counts.items():
        if rows:
            print(f"  rows {key} = {rows}")

    if not args.apply:
        print("dry run - re-run with --apply to perform the rename")
        return 0

    account_data = accounts_module.account_context.BOT_DIR / "account_data"
    for old, new in mapping.items():
        src, dst = account_data / old, account_data / new
        if src.is_dir() and not dst.exists():
            src.rename(dst)
            print(f"  moved {src.name}/ -> {dst.name}/")
        elif src.is_dir():
            print(f"  note: {dst.name}/ exists; left {src.name}/ in place")

    print("migration applied")
    return 0


def cmd_db_status(args: argparse.Namespace) -> int:
    with db_module.connection() as conn:
        print(f"database={db_module.database_name()}")
        known = db_module.list_accounts_in_db(conn)
        if not known:
            print("no rbc rows for any account yet; run `db populate`")
        for aid, rbc_rows in known:
            print(f"db_account aid={aid} rbc_rows={rbc_rows}")
        if args.account:
            account = accounts_module.load_account(args.account)
            _print_counts(conn, account.aid, account.name)
    return 0


def cmd_db_populate(args: argparse.Namespace) -> int:
    from bot import populate_database_rbc

    argv = ["--account-name", args.account]
    if args.logs_dir is not None:
        argv += ["--logs-dir", str(args.logs_dir)]
    if args.force:
        argv.append("--force")
    if getattr(args, "watch", False):
        argv.append("--watch")
        argv += ["--poll", str(args.poll)]
    return populate_database_rbc.main(argv)


# ---------------------------------------------------------------------------
# Runtime commands (delegate to the existing entry points)
# ---------------------------------------------------------------------------


def cmd_proxy(args: argparse.Namespace) -> int:
    from bot import proxy_bot

    forwarded = list(args.forwarded)
    # `python -m bot.cli proxy start` is nicer than `... proxy --start`, so accept
    # the subcommand form and translate it for the driver's own parser.
    if forwarded and forwarded[0] in {"start", "end", "status"}:
        forwarded[0] = f"--{forwarded[0]}"
    return proxy_bot.main(["--account-name", args.account, *forwarded])


def cmd_run(args: argparse.Namespace) -> int:
    from bot import bot as core

    account = accounts_module.load_account(args.account)
    return core.main(["--account-name", account.name, *args.forwarded])


# ---------------------------------------------------------------------------
# Argument wiring
# ---------------------------------------------------------------------------


def build_parser() -> tuple[argparse.ArgumentParser, argparse.ArgumentParser]:
    parser = argparse.ArgumentParser(
        prog="python -m bot.cli",
        description="Empire bot operator CLI.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    accounts_module.add_account_arguments(parser)

    subparsers = parser.add_subparsers(dest="command", required=True)

    subparsers.add_parser("accounts", help="list accounts discovered from credentials/*.env")
    subparsers.add_parser("session", help="which account is logged in right now (from the listener)")
    subparsers.add_parser("tasks", help="show the task plan").add_argument(
        "--plan", action="store_true", help="also show the current kingdom plan"
    )
    subparsers.choices["tasks"].add_argument(
        "--all", action="store_true", help="show the default plan plus every account's plan"
    )
    subparsers.add_parser("attacks", help="show the attack registry").add_argument(
        "--payload", action="store_true", help="dump the full JSON payload for each attack"
    )
    subparsers.add_parser("troops", help="list troop and tool ids for composing attacks")

    db_parser = subparsers.add_parser("db", help="create and inspect the database")
    db_sub = db_parser.add_subparsers(dest="db_command", required=True)

    db_sub.add_parser("init", help="create the database if missing and apply the schema")
    db_sub.add_parser("schema", help="apply the schema to an existing database")
    db_sub.add_parser("status", help="show per-account row counts")
    p_migrate = db_sub.add_parser(
        "migrate-keys",
        help="rename legacy numeric account keys onto login-name keys",
    )
    p_migrate.add_argument("--apply", action="store_true", help="perform the rename (default is a dry run)")

    p_logs = db_sub.add_parser("logs", help="show capture folder sizes and prune them")
    p_logs.add_argument("--folder", type=Path, default=None, help="inspect one folder instead of all")
    p_logs.add_argument("--prune", action="store_true", help="delete already-processed captures above the cap")
    p_logs.add_argument("--apply", action="store_true", help="actually delete (with --prune; default is a dry run)")
    p_logs.add_argument(
        "--force",
        action="store_true",
        help="with --prune: delete oldest-first even if untracked. Requires --folder.",
    )

    p_populate = db_sub.add_parser("populate", help="load RBC targets for an account from its capture logs")
    p_populate.add_argument(
        "--logs-dir",
        type=Path,
        default=None,
        help="Capture directory. Defaults to this account's own bot/account_data/<aid>/logs.",
    )
    p_populate.add_argument("--force", action="store_true", help="reprocess logs even if their checksum was seen")
    p_populate.add_argument(
        "--watch",
        action="store_true",
        help="keep running and keep ingesting new captures until Ctrl+C",
    )
    p_populate.add_argument(
        "--poll",
        type=float,
        default=10.0,
        metavar="SECONDS",
        help="with --watch: seconds between scans (default 10)",
    )

    proxy_parser = subparsers.add_parser("proxy", help="start/stop the proxy driver")
    proxy_parser.add_argument("forwarded", nargs=argparse.REMAINDER, help="arguments forwarded to bot.proxy_bot")

    run_parser = subparsers.add_parser("run", help="run the direct (non-proxy) runner")
    run_parser.add_argument("forwarded", nargs=argparse.REMAINDER, help="arguments forwarded to bot.bot")

    return parser, subparsers


#: Commands that never need an account selected.
_ACCOUNT_OPTIONAL = {"accounts", "session", "tasks", "attacks", "troops"}


def _resolve_account(args: argparse.Namespace, *, required: bool, parser: argparse.ArgumentParser) -> str | None:
    """Explicit flag wins, otherwise fall back to the detected login session."""

    if args.account_name:
        return accounts_module.load_account(args.account_name).name

    account = accounts_module.session_account()
    if account is not None:
        return account.name

    if required:
        choices = ", ".join(f"--{name}" for name in accounts_module.available_accounts())
        parser.error(
            "no account is logged in yet. Start the listener and log in so the session "
            f"is detected, or pass an explicit account: {choices or 'none found'}"
        )
    return None


def main(argv: list[str] | None = None) -> int:
    parser, _ = build_parser()
    raw = list(sys.argv[1:] if argv is None else argv)
    args = parser.parse_args(accounts_module.apply_account_flags(raw))
    command = args.command

    if command == "db":
        # `db init|schema|status` must work with no login at all.
        args.account = _resolve_account(
            args,
            required=args.db_command == "populate",
            parser=parser,
        )
        return {
            "init": cmd_db_init,
            "schema": cmd_db_schema,
            "status": cmd_db_status,
            "populate": cmd_db_populate,
            "migrate-keys": cmd_db_migrate_keys,
            "logs": cmd_db_logs,
        }[args.db_command](args)

    if command in _ACCOUNT_OPTIONAL:
        args.account = None
    else:
        # Long-running commands must never guess: session or explicit flag only.
        args.account = _resolve_account(args, required=True, parser=parser)

    return {
        "accounts": cmd_accounts,
        "session": cmd_session,
        "attacks": cmd_attacks,
        "tasks": cmd_tasks,
        "troops": cmd_troops,
        "proxy": cmd_proxy,
        "run": cmd_run,
    }[command](args)


if __name__ == "__main__":
    raise SystemExit(main())
