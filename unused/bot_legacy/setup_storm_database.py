from __future__ import annotations

import sys
from pathlib import Path


def find_repo_root(path: Path) -> Path:
    for parent in (path, *path.parents):
        if (parent / "bot" / "scheduler.py").is_file():
            return parent
    return path.parents[1]


REPO_ROOT = find_repo_root(Path(__file__).resolve())
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from bot import bot as core
from bot.storm_database import cleanup_expired_storm_targets, ensure_storm_tables
from bot.test_psql_connection import connect, read_connection_config


def main(argv: list[str] | None = None) -> int:
    account_name = (argv or sys.argv[1:] or ["ventrilo"])[0]
    core.configure_account(account_name)
    with connect(read_connection_config()) as conn:
        ensure_storm_tables(conn)
        deleted = cleanup_expired_storm_targets(conn)
        with conn.cursor() as cur:
            cur.execute("SELECT count(*) FROM storm_target WHERE aid = %s", (core.current_aid(),))
            total = int(cur.fetchone()[0])
            cur.execute(
                "SELECT count(*) FROM storm_target WHERE aid = %s AND expires_at > extract(epoch from now())::bigint",
                (core.current_aid(),),
            )
            live = int(cur.fetchone()[0])
        print(f"storm database ready table=storm_target total={total} live={live} expired_deleted={deleted}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
