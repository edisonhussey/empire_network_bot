from __future__ import annotations

import argparse
import json
import sys
import time
from collections import Counter
from pathlib import Path
from typing import Any


def find_repo_root(path: Path) -> Path:
    for parent in (path, *path.parents):
        if (parent / "bot" / "scheduler.py").is_file():
            return parent
    return path.parents[1]


REPO_ROOT = find_repo_root(Path(__file__).resolve())
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from bot.packets import extract_raw_packet, parse_xt_packet


CONTROL_FILE = REPO_ROOT / "bot" / "proxy_control.json"
CONTROL_LOG = REPO_ROOT / "bot" / "rbc_proxy_listener.log"
LOGS_DIR = REPO_ROOT / "bot" / "logs"
STORM_KID = 4
MAP_CHUNK_SIZE = 13


def load_json(path: Path, default: dict[str, Any]) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError):
        return default.copy()
    return data if isinstance(data, dict) else default.copy()


def save_json(path: Path, data: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    tmp.replace(path)


def iter_recent_blocks(limit_files: int = 200):
    paths = sorted(LOGS_DIR.glob("*.log"), key=lambda item: item.stat().st_mtime, reverse=True)
    for path in paths[:limit_files]:
        text = path.read_text(encoding="utf-8", errors="replace")
        for block in reversed(text.split("---")):
            yield path, block


def latest_client_gaa(kid: int) -> tuple[int, int, str | None] | None:
    for _path, block in iter_recent_blocks():
        if "CLIENT -> SERVER" not in block or "%gaa%" not in block:
            continue
        packet = extract_raw_packet(block)
        if packet is None:
            continue
        parsed = parse_xt_packet(packet)
        payload = parsed.get("payload") if parsed else None
        if not isinstance(payload, dict) or parsed.get("command") != "gaa":
            continue
        try:
            if int(payload.get("KID")) != kid:
                continue
            return int(payload["AX1"]), int(payload["AY1"]), parsed.get("server_header")
        except (KeyError, TypeError, ValueError):
            continue
    return None


def latest_server_gaa(kid: int, since_epoch: float) -> tuple[Path, dict[str, Any]] | None:
    paths = sorted(LOGS_DIR.glob("*.log"), key=lambda item: item.stat().st_mtime, reverse=True)
    for path in paths[:80]:
        if path.stat().st_mtime + 2 < since_epoch:
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        for block in reversed(text.split("---")):
            if "SERVER -> CLIENT" not in block or "%gaa%" not in block:
                continue
            packet = extract_raw_packet(block)
            if packet is None:
                continue
            parsed = parse_xt_packet(packet)
            payload = parsed.get("payload") if parsed else None
            if not isinstance(payload, dict) or parsed.get("command") != "gaa":
                continue
            try:
                if int(payload.get("KID")) == kid:
                    return path, payload
            except (TypeError, ValueError):
                continue
    return None


def summarize_gaa(payload: dict[str, Any]) -> str:
    rows = [row for row in payload.get("AI") or [] if isinstance(row, list) and row]
    area_counts = Counter()
    row_lengths = Counter()
    examples: dict[int, list[Any]] = {}
    for row in rows:
        try:
            area_type = int(row[0])
        except (TypeError, ValueError):
            continue
        area_counts[area_type] += 1
        row_lengths[(area_type, len(row))] += 1
        examples.setdefault(area_type, row[:12])

    lines = [
        f"server_gaa kid={payload.get('KID')} rows={len(rows)}",
        "area_counts=" + ", ".join(f"{key}:{area_counts[key]}" for key in sorted(area_counts)),
        "row_lengths=" + ", ".join(
            f"type{area_type}/len{length}:{count}"
            for (area_type, length), count in sorted(row_lengths.items())
        ),
    ]
    for area_type in sorted(examples):
        lines.append(f"example_type_{area_type}={examples[area_type]}")
    return "\n".join(lines)


def queue_probe(args: argparse.Namespace) -> dict[str, Any]:
    if args.ax1 is None or args.ay1 is None:
        latest = latest_client_gaa(args.kid)
        if latest is None:
            raise SystemExit("no recent client Storm gaa found; pass --ax1 and --ay1")
        ax1, ay1, server_header = latest
    else:
        ax1, ay1, server_header = args.ax1, args.ay1, args.server_header

    state = load_json(CONTROL_FILE, {"running": False, "max_attacks": 0, "attacks_sent": 0})
    pending = state.get("pending")
    if pending and not args.force:
        raise SystemExit(f"control file already has pending={pending!r}; use --force to replace it")

    probe = {
        "kind": "gaa_probe",
        "kid": int(args.kid),
        "ax1": int(ax1),
        "ay1": int(ay1),
        "ax2": int(ax1) + MAP_CHUNK_SIZE - 1,
        "ay2": int(ay1) + MAP_CHUNK_SIZE - 1,
        "queued_at": time.time(),
    }
    if server_header:
        probe["server_header"] = str(server_header)
    state["pending"] = probe
    save_json(CONTROL_FILE, state)
    return probe


def wait_for_send(queued_at: float, timeout: float) -> dict[str, Any] | None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        state = load_json(CONTROL_FILE, {})
        last = state.get("last_gaa_probe")
        if isinstance(last, dict) and float(last.get("sent_at", 0) or 0) >= queued_at - 1:
            return last
        time.sleep(0.5)
    return None


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Queue one Storm GAA probe through the active mitmproxy listener.")
    parser.add_argument("--kid", type=int, default=STORM_KID)
    parser.add_argument("--ax1", type=int)
    parser.add_argument("--ay1", type=int)
    parser.add_argument("--server-header")
    parser.add_argument("--timeout", type=float, default=20.0)
    parser.add_argument("--force", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    queued = queue_probe(args)
    print(
        "queued gaa_probe "
        f"kid={queued['kid']} chunk={queued['ax1']}:{queued['ay1']}-{queued['ax2']}:{queued['ay2']}"
    )

    sent = wait_for_send(float(queued["queued_at"]), args.timeout)
    if sent is None:
        print("probe was not marked sent; mitmdump may need to reload/restart the patched listener")
        return 2
    print(f"sent gaa_probe kid={sent['kid']} chunk={sent['ax1']}:{sent['ay1']}-{sent['ax2']}:{sent['ay2']}")

    deadline = time.time() + args.timeout
    while time.time() < deadline:
        found = latest_server_gaa(args.kid, float(queued["queued_at"]))
        if found is not None:
            path, payload = found
            print(f"response_log={path}")
            print(summarize_gaa(payload))
            return 0
        time.sleep(0.5)
    print("probe sent, but no matching server gaa response was found in the capture logs yet")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
