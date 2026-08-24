from __future__ import annotations

import sys
from pathlib import Path


def find_repo_root(path: Path) -> Path:
    for parent in (path, *path.parents):
        if (parent / "empire").is_dir():
            return parent
    return path.parents[1]


REPO_ROOT = find_repo_root(Path(__file__).resolve())
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from empire.bot.storm_database import cleanup_expired_storm_targets, ensure_storm_tables
from empire.bot.test_psql_connection import connect, read_connection_config


def main() -> int:
    with connect(read_connection_config()) as conn:
        ensure_storm_tables(conn)
        deleted = cleanup_expired_storm_targets(conn)
        with conn.cursor() as cur:
            cur.execute("SELECT count(*) FROM storm_target")
            total = int(cur.fetchone()[0])
            cur.execute("SELECT count(*) FROM storm_target WHERE expires_at > extract(epoch from now())::bigint")
            live = int(cur.fetchone()[0])
        print(f"storm database ready table=storm_target total={total} live={live} expired_deleted={deleted}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
