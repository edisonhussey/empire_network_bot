"""Run hands-off Berimond farming from the mode-local ``run_config.json``.

The mode-local controller remains the diagnostic CLI; this is the intentionally
boring production entry point. Edit one JSON file,
start mitmdump, then run ``python run_berimond.py``.  It never starts Sands.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

from bot.event.berimond_kingdom import controller as bot_berimond


DEFAULT_CONFIG = Path(__file__).parent / "bot" / "event" / "berimond_kingdom" / "run_config.json"


def _option(name: str) -> str:
    return "--" + name.replace("_", "-")


def build_argv(config: dict[str, Any]) -> list[str]:
    account = str(config.get("account") or "").strip()
    if not account:
        raise ValueError("berimond config requires a non-empty 'account'")

    common = ["--account-name", account]
    for key in ("commander_count", "source", "minutes", "log_files"):
        value = config.get(key)
        if value is not None:
            common.extend([_option(key), str(value)])
    if config.get("no_db"):
        common.append("--no-db")

    run = ["run"]
    scalar_keys = (
        "target", "pick", "max_attacks", "hbw", "ptt", "global_cooldown",
        "max_commander_out", "find", "probe_every", "error_tolerance",
        "error_pause", "army_backoff", "refill_units", "refill_safety",
        "refill_castle_id", "refill_source_kingdom", "refill_wait",
        "refill_max_wait", "refill_stock_max_age", "skip_tolerance", "radius",
        "refill_time_skip_type", "refill_max_time_skips", "stall_probes",
        "idle_sleep", "startup_timeout", "cache_max_age",
    )
    for key in scalar_keys:
        value = config.get(key)
        if value is not None:
            run.extend([_option(key), str(value)])
    run.append("--refill" if config.get("refill", True) else "--no-refill")
    run.append("--refill-time-skip" if config.get("refill_time_skip", True) else "--no-refill-time-skip")
    if config.get("no_probe"):
        run.append("--no-probe")
    return [*common, *run]


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    config_path = Path(args[0]).expanduser() if args else DEFAULT_CONFIG
    try:
        config = json.loads(config_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise SystemExit(f"cannot load Berimond config {config_path}: {exc}") from exc
    if not isinstance(config, dict):
        raise SystemExit(f"Berimond config must be a JSON object: {config_path}")
    print(f"berimond_config={config_path} account={config.get('account')} mode=berimond")
    return bot_berimond.main(build_argv(config))


if __name__ == "__main__":
    raise SystemExit(main())
