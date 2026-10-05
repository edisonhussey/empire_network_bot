#!/usr/bin/env python3
"""Send a short burst of fortress probes at a chosen tempo and report what came back.

The question this answers is *the server's* tolerance, not the runner's pacing:
if ten `gaa` windows go out 0.23 s apart, do ten replies come back? Injecting
through `/v1/injections` puts the requests on the live socket without a rebuild,
so the tempo can be tried long before it is committed to.

Ten requests is deliberately small — enough to see whether the stream stalls,
small enough not to look like an attack.

    python3 development/lab/pace_probe.py --dry-run
    python3 development/lab/pace_probe.py --connect        # open a session first
    python3 development/lab/pace_probe.py --count 10 --base 0.23

A record of every run is appended to `development/logs/pace-YYYYMMDD.log`, one
line per request plus a summary, because `network_message` is a rolling buffer
and the evidence has to outlive it.

Standard library only. Never prints credentials.
"""

from __future__ import annotations

import argparse
import json
import random
import sqlite3
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
LOG_DIR = ROOT / "development" / "logs"
DAEMON = "http://127.0.0.1:47821/v1"
DB_PATH = Path.home() / "Library/Application Support/com.openauto.desktop/empire.sqlite3"

# Mirrors crates/empire-core/src/fortress.rs. Duplicated on purpose: this is a
# throwaway probe and must keep working even if the sweep model changes.
STEP = 39
OFFSETS = (9, 29)
MAX_COORD = 1285
SLOTS_PER_FAMILY = 2
BLOCK_SPAN = STEP * (SLOTS_PER_FAMILY - 1) + (OFFSETS[1] - OFFSETS[0])  # 59
BLOCK_STEP = STEP * SLOTS_PER_FAMILY  # 78
SWEEP_ALIGNMENT = OFFSETS[0]  # 9
GAA_PAD = 1
SERVER_HEADER = "EmpireEx_21"

# Ten disjoint windows in the densest ground known in Sands, so each reply's
# objects identify which request answered them.
ORIGINS = [
    (633, 633), (633, 555), (555, 633), (633, 711), (711, 633),
    (555, 555), (633, 477), (477, 633), (711, 555), (555, 711),
]

SLOW_CHANCE = 0.10


def window(origin: tuple[int, int]) -> tuple[int, int, int, int]:
    x, y = origin
    return (x - GAA_PAD, y - GAA_PAD, x + BLOCK_SPAN + GAA_PAD, y + BLOCK_SPAN + GAA_PAD)


def gaa_packet(kingdom_id: int, bounds: tuple[int, int, int, int]) -> str:
    ax1, ay1, ax2, ay2 = bounds
    body = json.dumps(
        {"KID": kingdom_id, "AX1": ax1, "AY1": ay1, "AX2": ax2, "AY2": ay2},
        separators=(",", ":"),
    )
    return f"%xt%{SERVER_HEADER}%gaa%1%{body}%"


def next_delay(rng: random.Random, base: float) -> float:
    """Nine gaps near `base`, the tenth visibly longer.

    The tail is the point: a stream of identical gaps is a metronome, and no
    client produces one.
    """
    if rng.random() < SLOW_CHANCE:
        return rng.uniform(base * 1.4, base * 2.2)
    return rng.uniform(base * 0.87, base * 1.13)


def call(method: str, path: str, body: dict | None = None, timeout: float = 10.0):
    url = path if path.startswith("http") else f"{DAEMON}{path}"
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(url, data=data, method=method)
    if data is not None:
        request.add_header("content-type", "application/json")
    with urllib.request.urlopen(request, timeout=timeout) as response:
        raw = response.read()
    return json.loads(raw) if raw else None


def read_env(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip().strip('"').strip("'")
    return values


def connect_if_needed(account: str) -> bool:
    """Sign the account in if no transport is up.

    Only Sands is enabled in the plan sent, so the running service does no
    fortress walk while the probes are going out — otherwise its own `gaa`
    traffic would be counted as replies.
    """
    health = call("GET", "/health")
    if health.get("transport_connected"):
        print(f"  already connected (api_version {health.get('api_version')})")
        return True

    creds = read_env(ROOT / "credentials" / f"{account}.env")
    if not creds.get("USERNAME") or not creds.get("PASSWORD"):
        print(f"  {account}.env has no USERNAME/PASSWORD; connect from the app instead", file=sys.stderr)
        return False
    print(f"  signing in as {creds['USERNAME']} (password not shown)")
    call(
        "POST",
        "/accounts",
        {
            "server": creds.get("SERVER", "US1"),
            "username": creds["USERNAME"],
            "password": creds["PASSWORD"],
            "scan_radius": 0,
            "kingdom_scans": [{"kingdom_id": 1, "enabled": True, "radius": 0}],
            "reuse_existing_map": True,
        },
    )
    deadline = time.time() + 120
    # `/direct` keeps reporting the *previous* attempt for a second or two after
    # the sign-in is posted. Reading that stale `failed` as this attempt's result
    # aborts a connect that is succeeding — which it did, once, and cost a run.
    grace_until = time.time() + 15
    while time.time() < deadline:
        status = call("GET", "/direct")
        if status.get("connected") and status.get("phase") == "sands_ready":
            print(f"  connected, phase {status['phase']}")
            return True
        if status.get("phase") == "failed" and time.time() > grace_until:
            print(f"  connect failed: {status.get('error')}", file=sys.stderr)
            return False
        time.sleep(2)
    print("  timed out waiting for sands_ready", file=sys.stderr)
    return False


def in_window(point: tuple[int, int], bounds: tuple[int, int, int, int]) -> bool:
    ax1, ay1, ax2, ay2 = bounds
    return ax1 <= point[0] <= ax2 and ay1 <= point[1] <= ay2


def predict_from_db(kingdom_id: int, windows: list[tuple[int, int, int, int]]):
    """Fortresses each window ought to contain, from what is already learned.

    Read-only and best-effort: a missing or locked database means no prediction,
    not a failed run.
    """
    if not DB_PATH.exists():
        return None
    try:
        con = sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True)
        rows = con.execute(
            "SELECT x, y FROM fortress_target WHERE kingdom_id = ?", (kingdom_id,)
        ).fetchall()
        con.close()
    except sqlite3.Error as error:
        print(f"  (no prediction: {error})", file=sys.stderr)
        return None
    return [[(int(x), int(y)) for x, y in rows if in_window((x, y), w)] for w in windows]


def returned_fortresses(entry: dict) -> list[tuple[int, int]]:
    """Type-11 coordinates out of one `gaa` reply."""
    rows = (entry.get("payload") or {}).get("AI") or []
    return [
        (int(r[1]), int(r[2]))
        for r in rows
        if isinstance(r, list) and len(r) >= 3 and r[0] == 11
        and isinstance(r[1], int) and isinstance(r[2], int)
    ]


def object_count(entry: dict) -> int:
    return len((entry.get("payload") or {}).get("AI") or [])


# Four distinct areas, one per window width, deliberately scrambled: if replies
# come back in send order the server is FIFO, and if they come back ordered by
# area it is not. Distinct areas are what makes the ordering readable — the
# contents identify which request a reply belongs to even when it arrives early.
LADDER = [
    ((477, 477), 4),   # widest window placed in the dense fortress ground
    ((750, 750), 1),
    ((87, 87), 3),
    ((867, 867), 2),
]


def ladder_window(origin: tuple[int, int], families: int) -> tuple[int, int, int, int]:
    x, y = origin
    span = STEP * (families - 1) + (OFFSETS[1] - OFFSETS[0])
    return (x - GAA_PAD, y - GAA_PAD, x + span + GAA_PAD, y + span + GAA_PAD)


def run_ladder(args) -> int:
    """One window per width, widely spaced, to separate latency from throughput.

    If the time per request is the same at every width then ~0.9 s is a fixed
    per-request cost (pacing, or a round trip), and a wider window is pure win.
    If it grows with the window then the cost is the work of scanning the
    rectangle, and widening buys much less than the probe count suggests.
    """
    status = call("GET", "/direct")
    if status.get("current_kingdom_id") != args.kingdom or not status.get("map_mode"):
        print(f"  not in map view for kingdom {args.kingdom}; open the Sands map first", file=sys.stderr)
        return 1

    plan = []
    for round_index in range(2):
        for origin, families in LADDER:
            plan.append((origin, families, round_index))
    rng = random.Random(args.seed)
    rng.shuffle(plan)

    spacing = 3.0  # far above the ~0.9 s service time, so nothing queues
    baseline = call("GET", "/messages") or []
    start_sequence = max((m.get("sequence", 0) for m in baseline), default=0)

    sent = []
    t0 = time.time()
    for index, (origin, families, _) in enumerate(plan):
        bounds = ladder_window(origin, families)
        call("POST", "/injections", {"packet": gaa_packet(args.kingdom, bounds), "ttl_ms": 20000})
        sent.append({"index": index, "origin": origin, "families": families,
                     "bounds": bounds, "at": time.time() - t0})
        if index < len(plan) - 1:
            time.sleep(spacing)

    deadline = time.time() + 20
    replies = []
    while time.time() < deadline:
        rows = call("GET", "/messages") or []
        fresh = [m for m in rows
                 if m.get("sequence", 0) > start_sequence
                 and m.get("direction") == "server_to_client"
                 and m.get("command") == "gaa"]
        if len(fresh) >= len(sent):
            replies = fresh
            break
        replies = fresh
        time.sleep(0.4)

    # Attribute by content: every object in a reply must sit inside one window,
    # and the four areas are disjoint, so this is unambiguous even out of order.
    print(f"\nladder — one window per width, {spacing:.0f} s apart, sent in scrambled order")
    print(f"  {'#':>2} {'families':>8} {'cells':>6} {'objects':>8} {'fortress':>9} "
          f"{'latency s':>10}  area")
    by_index = {}
    for entry in sorted(replies, key=lambda m: m.get("sequence", 0)):
        points = returned_fortresses(entry) or []
        payload = (entry.get("payload") or {}).get("AI") or []
        coords = [(int(r[1]), int(r[2])) for r in payload
                  if isinstance(r, list) and len(r) >= 3
                  and isinstance(r[1], int) and isinstance(r[2], int)]
        owner = None
        for item in sent:
            if item["index"] in by_index:
                continue
            if coords and all(in_window(c, item["bounds"]) for c in coords):
                owner = item
                break
        if owner is None:
            continue
        by_index[owner["index"]] = entry
        latency = (entry["observed_at_ms"] / 1000.0) - (t0 + owner["at"])
        width = owner["bounds"][2] - owner["bounds"][0] + 1
        print(f"  {owner['index']:>2} {owner['families']:>8} {width:>6} "
              f"{object_count(entry):>8} {len(points):>9} {latency:>10.3f}  "
              f"{'x'.join(str(v) for v in owner['origin'])}")

    order_sent = [i["index"] for i in sent if i["index"] in by_index]
    reply_order = sorted(by_index, key=lambda i: by_index[i]["sequence"])
    print(f"\n  sent order : {order_sent}")
    print(f"  reply order: {reply_order}")
    print(f"  FIFO (replies follow send order): {'YES' if order_sent == reply_order else 'NO'}")

    by_width = {}
    for index, entry in by_index.items():
        item = next(i for i in sent if i["index"] == index)
        by_width.setdefault(item["families"], []).append(
            ((entry["observed_at_ms"] / 1000.0) - (t0 + item["at"]), object_count(entry))
        )
    print("\n  width    mean latency   mean objects")
    for families in sorted(by_width):
        pairs = by_width[families]
        mean_latency = sum(p[0] for p in pairs) / len(pairs)
        mean_objects = sum(p[1] for p in pairs) / len(pairs)
        print(f"  {families:>5}    {mean_latency:>12.3f}   {mean_objects:>12.0f}")

    LOG_DIR.mkdir(parents=True, exist_ok=True)
    log = LOG_DIR / f"ladder-{datetime.now():%Y%m%d}.log"
    with log.open("a") as handle:
        handle.write(json.dumps({
            "at": datetime.now().isoformat(timespec="seconds"),
            "fifo": order_sent == reply_order,
            "spacing": spacing,
            "widths": {str(k): round(sum(p[0] for p in v) / len(v), 3) for k, v in by_width.items()},
        }) + "\n")
    print(f"\nrecorded to {log.relative_to(ROOT)}")
    return 0


def classify(entry: dict, windows: list[tuple[int, int, int, int]]) -> tuple[str, int, str]:
    """Which window a reply's objects fall inside, how many, and the extent."""
    payload = entry.get("payload") or {}
    rows = payload.get("AI") or []
    points = [r for r in rows if isinstance(r, list) and len(r) >= 3][:5000]
    coords = [(int(r[1]), int(r[2])) for r in points if isinstance(r[1], int) and isinstance(r[2], int)]
    fortresses = sum(1 for r in points if r and r[0] == 11)
    if not coords:
        return ("empty", 0, "-")
    extent = f"x {min(c[0] for c in coords)}..{max(c[0] for c in coords)} " \
             f"y {min(c[1] for c in coords)}..{max(c[1] for c in coords)}"
    hits = [i for i, w in enumerate(windows) if all(in_window(c, w) for c in coords)]
    label = f"window {hits[0]}" if len(hits) == 1 else ("spanning" if not hits else "overlap")
    return (label, fortresses, extent)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--count", type=int, default=10, help="probes to send (default 10)")
    parser.add_argument("--base", type=float, default=0.23, help="baseline gap in seconds (default 0.23)")
    parser.add_argument("--observe", type=float, default=20.0, help="seconds to keep watching after the last send")
    parser.add_argument("--kingdom", type=int, default=1)
    parser.add_argument("--account", default="ventrilo")
    parser.add_argument("--connect", action="store_true", help="sign in first if no transport is up")
    parser.add_argument("--verify", action="store_true",
                        help="predict each window from the database and compare")
    parser.add_argument("--ladder", action="store_true",
                        help="one window per width, widely spaced: latency vs throughput")
    parser.add_argument("--dry-run", action="store_true", help="print the plan and send nothing")
    parser.add_argument("--seed", type=int, default=None)
    args = parser.parse_args()

    if args.ladder:
        return run_ladder(args)

    rng = random.Random(args.seed)
    taken = min(args.count, len(ORIGINS))
    windows = [window(o) for o in ORIGINS[:taken]]
    delays = [next_delay(rng, args.base) for _ in range(taken - 1)]
    plan_seconds = sum(delays)

    print(f"fortress pacing probe — {taken} probes, baseline {args.base:.2f} s")
    print(f"  window span {BLOCK_SPAN} (cells {BLOCK_SPAN + 2 * GAA_PAD + 1}), "
          f"step {BLOCK_STEP}, alignment {SWEEP_ALIGNMENT}")
    print(f"  planned gaps: min {min(delays):.3f}  mean {sum(delays)/len(delays):.3f}  "
          f"max {max(delays):.3f}  total {plan_seconds:.1f} s")
    print(f"  windows: {windows[0]} .. {windows[-1]}")
    if args.dry_run:
        for index, (origin, bounds) in enumerate(zip(ORIGINS[:taken], windows)):
            print(f"    {index:2d}  origin {origin}  {bounds}")
        print("  dry run: nothing sent")
        return 0

    health = call("GET", "/health") if not args.connect else None
    if args.connect:
        if not connect_if_needed(args.account):
            return 1
    elif not health.get("transport_connected"):
        print("  no transport: run with --connect, or press Start in the app", file=sys.stderr)
        return 1

    # The window is only honoured in map view. Asking from castle view returns a
    # castle, not a map, and every number below would be meaningless.
    status = call("GET", "/direct")
    if status.get("current_kingdom_id") != args.kingdom or not status.get("map_mode"):
        print(
            f"  not in map view for kingdom {args.kingdom} "
            f"(kingdom={status.get('current_kingdom_id')} map_mode={status.get('map_mode')}); "
            "open the Sands map first",
            file=sys.stderr,
        )
        return 1
    print(f"  in map view for kingdom {args.kingdom}")

    expected = predict_from_db(args.kingdom, windows) if args.verify else None
    if expected is not None:
        print(f"  predicted from the database: "
              f"{[len(e) for e in expected]} fortresses per window")

    baseline = call("GET", "/messages") or []
    start_sequence = max((m.get("sequence", 0) for m in baseline), default=0)
    print(f"  baseline sequence {start_sequence}")

    sent: list[tuple[int, float, tuple[int, int, int, int]]] = []
    t0 = time.time()
    for index, bounds in enumerate(windows):
        packet = gaa_packet(args.kingdom, bounds)
        try:
            call("POST", "/injections", {"packet": packet, "ttl_ms": 15000})
        except urllib.error.HTTPError as error:
            print(f"  inject {index} rejected: {error.code} {error.read()[:120]!r}", file=sys.stderr)
            break
        sent.append((index, time.time() - t0, bounds))
        if index < len(delays):
            time.sleep(delays[index])
    send_done = time.time() - t0
    print(f"  sent {len(sent)} in {send_done:.2f} s")

    # Keep watching past the last send: a reply may be slow, and a stall shows up
    # as replies that never arrive rather than as a fast failure.
    deadline = time.time() + args.observe
    replies: list[dict] = []
    while time.time() < deadline:
        rows = call("GET", "/messages") or []
        fresh = [
            m for m in rows
            if m.get("sequence", 0) > start_sequence
            and m.get("direction") == "server_to_client"
            and m.get("command") == "gaa"
        ]
        if len(fresh) >= len(sent):
            replies = fresh
            break
        replies = fresh
        time.sleep(0.5)

    print(f"\n{len(replies)} reply/replies for {len(sent)} request(s)")
    records = []
    returned: dict[int, set[tuple[int, int]]] = {}
    for entry in sorted(replies, key=lambda m: m.get("sequence", 0)):
        label, fortresses, extent = classify(entry, windows)
        if label.startswith("window "):
            returned.setdefault(int(label.split()[1]), set()).update(returned_fortresses(entry))
        if not args.verify:
            print(f"    seq {entry.get('sequence'):>5}  {label:<10} "
                  f"{fortresses:>3} fortress  {extent}")
        records.append({
            "sequence": entry.get("sequence"),
            "observed_at_ms": entry.get("observed_at_ms"),
            "window": label,
            "fortresses": fortresses,
            "extent": extent,
        })

    missing_total = 0
    if expected is not None:
        print("\n  window     expected  returned  missing  extra")
        for index, (want, bounds) in enumerate(zip(expected, windows)):
            got = returned.get(index, set())
            missing = set(want) - got
            extra = got - set(want)
            missing_total += len(missing)
            flag = "" if not missing and not extra else "   <-- MISMATCH"
            print(f"  {index:>4} {bounds[0]:>5},{bounds[1]:<5} {len(want):>7} {len(got):>9} "
                  f"{len(missing):>8} {len(extra):>6}{flag}")
            if missing:
                print(f"          missing: {sorted(missing)[:6]}")

    matched = sum(1 for r in records if r["window"].startswith("window"))
    observed_gaps = [
        (records[i]["observed_at_ms"] - records[i - 1]["observed_at_ms"]) / 1000.0
        for i in range(1, len(records))
    ]
    summary = {
        "at": datetime.now().isoformat(timespec="seconds"),
        "sent": len(sent),
        "replies": len(replies),
        "matched_to_a_window": matched,
        "missing_fortresses": missing_total if expected is not None else None,
        "send_seconds": round(send_done, 3),
        "planned_gap_mean": round(sum(delays) / len(delays), 3) if delays else None,
        "reply_gap_min": round(min(observed_gaps), 3) if observed_gaps else None,
        "reply_gap_mean": round(sum(observed_gaps) / len(observed_gaps), 3) if observed_gaps else None,
        "reply_gap_max": round(max(observed_gaps), 3) if observed_gaps else None,
        "base": args.base,
        "kingdom": args.kingdom,
    }
    print("\nsummary: " + json.dumps(summary))

    LOG_DIR.mkdir(parents=True, exist_ok=True)
    log = LOG_DIR / f"pace-{datetime.now():%Y%m%d}.log"
    with log.open("a") as handle:
        handle.write(json.dumps(summary) + "\n")
        for record in records:
            handle.write("  " + json.dumps(record) + "\n")
    print(f"recorded to {log.relative_to(ROOT)}")

    if len(replies) < len(sent):
        print(f"\nVERDICT: {len(sent) - len(replies)} of {len(sent)} went unanswered at "
              f"{args.base:.2f} s — the tempo is too fast.", file=sys.stderr)
        return 2
    print(f"\nVERDICT: all {len(sent)} answered at {args.base:.2f} s.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
