from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def find_repo_root(path: Path) -> Path:
    for parent in (path, *path.parents):
        if (parent / "bot" / "scheduler.py").is_file():
            return parent
    return path.parents[1]


REPO_ROOT = find_repo_root(Path(__file__).resolve())
CREDENTIALS_DIR = REPO_ROOT / "credentials"


def read_env(path: Path) -> dict[str, str]:
    """Read a simple ``KEY = VALUE`` credentials file."""

    values: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or "=" not in stripped:
            continue
        key, value = stripped.split("=", 1)
        values[key.strip()] = value.strip().strip("\"'")
    return values


def credentials_path(username: str) -> Path:
    return CREDENTIALS_DIR / f"{username}.env"


def available_usernames() -> tuple[str, ...]:
    """Every account that has a ``credentials/<username>.env`` file."""

    if not CREDENTIALS_DIR.is_dir():
        return ()
    return tuple(sorted(path.stem for path in CREDENTIALS_DIR.glob("*.env")))


def resolve_aid(username: str) -> str:
    path = credentials_path(username)
    if not path.exists():
        raise FileNotFoundError(f"missing credentials file: {path}")
    values = read_env(path)
    aid = values.get("AID") or values.get("ACCOUNT_ID") or values.get("account_id")
    if not aid:
        raise ValueError(f"missing AID in {path}")
    return str(aid)


def account_record(username: str) -> dict[str, str]:
    path = credentials_path(username)
    values = read_env(path)
    return {
        "username": username,
        "aid": resolve_aid(username),
        "server": values.get("SERVER", ""),
        "credentials": str(path),
    }


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Translate account username to Goodgame account id from credentials/*.env")
    parser.add_argument("username")
    parser.add_argument("--json", action="store_true", help="print username/aid/server metadata")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        record = account_record(args.username)
    except Exception as exc:
        print(f"aid_translation error: {exc}", file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(record, indent=2, sort_keys=True))
    else:
        print(record["aid"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
