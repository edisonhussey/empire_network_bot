#!/usr/bin/env python3
"""Minimal GGE network login skeleton.

This file is for learning the module shape only:
1. point Python at the local helper modules,
2. load one account .ini,
3. log in,
4. make one harmless read,
5. disconnect.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


DEFAULT_SCAN_ROOT = Path.home() / "Desktop" / "scan_coordinates"


def add_module_paths(scan_root: Path) -> None:
    """Expose sibling modules used by the existing bot code."""
    paths = (
        scan_root,
        scan_root / "pygge_repo",
    )
    for path in paths:
        if str(path) not in sys.path:
            sys.path.insert(0, str(path))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Log in once and print castle count.")
    parser.add_argument(
        "--scan-root",
        type=Path,
        default=DEFAULT_SCAN_ROOT,
        help="Folder containing event_worker/ and pygge_repo/.",
    )
    parser.add_argument(
        "--config",
        "-C",
        type=Path,
        required=True,
        help="Account .ini path, for example ~/Desktop/scan_coordinates/furox.ini.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    scan_root = args.scan_root.expanduser().resolve()
    config_path = args.config.expanduser().resolve()

    add_module_paths(scan_root)

    from event_worker.gge_session import connect_and_login, disconnect

    socket = connect_and_login(config_path=config_path, humanize=False)
    try:
        castles_response = socket.get_castles()
        data = castles_response.get("payload", {}).get("data") or {}
        kingdom_blocks = data.get("C", [])

        print(f"logged in using: {config_path}")
        print(f"kingdom blocks returned: {len(kingdom_blocks)}")
        return 0
    finally:
        disconnect(socket)


if __name__ == "__main__":
    raise SystemExit(main())
