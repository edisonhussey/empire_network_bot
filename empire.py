#!/usr/bin/env python
"""Run the bot CLI from any directory.

Prefer this over ``python -m bot.cli`` when you are not in the repo root.

Running ``python -m bot.cli`` from inside ``bot/`` fails confusingly:

    ModuleNotFoundError: No module named 'bot.test_psql_connection';
    'bot' is not a package

because that directory contains ``bot.py``, and Python looks in the current
directory first - so ``import bot`` picks up the standalone module instead of the
``bot`` package.

This script lives at the repo root and clears the current directory from
``sys.path`` before importing, so it works from anywhere:

    python empire.py db populate
    python empire.py proxy start --max-attacks 200
    cd bot && python ../empire.py session
"""

from __future__ import annotations

import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent

# Remove the current directory / script directory entries so a stray `bot.py`
# cannot shadow the `bot` package.
_shadow_candidates = {"", ".", str(Path.cwd()), str(REPO_ROOT / "bot")}
sys.path[:] = [entry for entry in sys.path if entry not in _shadow_candidates]

if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from bot.cli import main  # noqa: E402  (import must follow the sys.path fix)


if __name__ == "__main__":
    raise SystemExit(main())
