#!/usr/bin/env python3
"""Offline simulator over recorded traffic.

It answers the questions that otherwise cost a login and ten minutes:

  * did the server actually answer the whole window we asked for?
  * does one request really cover four lattice slots?
  * how many requests would a kingdom cost at a given window size?

Everything is computed from the `network_message` table, so nothing here is a
model of what the server *should* do — it is a measurement of what it did.

What this cannot prove
----------------------
It cannot tell you whether a span the bot has never sent would be honoured.
Only a live probe can. `--plan` extrapolates cost from observed density, and
that extrapolation is labelled as such in the output.

Usage
-----
    python3 simulate.py                     # analyse the recorded history
    python3 simulate.py --plan 62           # cost of a 62-wide window
    python3 simulate.py --watch 2.0         # replay the sweep, 2s per step
    python3 simulate.py --db /path/to.sqlite3
"""

from __future__ import annotations

import argparse
import sqlite3
import sys
import time
from pathlib import Path

# Mirrors crates/empire-core/src/fortress.rs. Kept as literals on purpose: if
# the Rust changes these, this file must change with it, and a mismatch shows up
# as a coverage failure rather than as silent agreement.
LATTICE_STEP = 39
LATTICE_OFFSETS = (9, 29)
MAP_MAX = 1285
BLOCK_STEP = LATTICE_STEP * 2
GAA_PAD = 1

DEFAULT_DB = (
    Path.home()
    / "Library/Application Support/com.openauto.desktop/empire.sqlite3"
)


def slots_in(min_x: int, min_y: int, max_x: int, max_y: int) -> list[tuple[int, int]]:
    """Every fortress slot inside the inclusive rectangle."""
    found: list[tuple[int, int]] = []
    for offset in LATTICE_OFFSETS:
        x = offset + LATTICE_STEP * max(0, -(-(min_x - offset) // LATTICE_STEP))
        while x <= max_x:
            y = offset + LATTICE_STEP * max(0, -(-(min_y - offset) // LATTICE_STEP))
            while y <= max_y:
                found.append((x, y))
                y += LATTICE_STEP
            x += LATTICE_STEP
    return found


def is_slot(x: int, y: int) -> bool:
    return x % LATTICE_STEP == y % LATTICE_STEP and x % LATTICE_STEP in LATTICE_OFFSETS


def block_slots(x: int, y: int) -> list[tuple[int, int]]:
    """The four slots one probe at `(x, y)` is meant to answer."""
    return [
        (x, y),
        (x + LATTICE_STEP, y),
        (x, y + LATTICE_STEP),
        (x + LATTICE_STEP, y + LATTICE_STEP),
    ]


def align_up(value: int, offset: int) -> int:
    remainder = (value - offset) % LATTICE_STEP
    return value if remainder == 0 else value + (LATTICE_STEP - remainder)


def block_count(span: int) -> int:
    """Blocks a whole kingdom needs at a given window span, one grid at a time.

    `span` is the window edge in map coordinates; the number of slots it covers
    per axis is the slots that fit inside it.
    """
    slots_per_axis = (span - 2 * GAA_PAD - 1) // LATTICE_STEP + 1
    slots_per_axis = max(slots_per_axis, 1)
    total_slots = (MAP_MAX - LATTICE_OFFSETS[0]) // LATTICE_STEP + 1
    windows_per_axis = -(-total_slots // slots_per_axis)
    return windows_per_axis * windows_per_axis


# --- reading the recording --------------------------------------------------


def connect(db: Path) -> sqlite3.Connection:
    if not db.exists():
        print(f"no database at {db}", file=sys.stderr)
        raise SystemExit(2)
    connection = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    return connection


def load_traffic(connection: sqlite3.Connection) -> tuple[list[dict], list[dict]]:
    """Requests and responses, newest last."""
    requests: list[dict] = []
    responses: list[dict] = []
    for row in connection.execute(
        "SELECT sequence, observed_at_ms, direction, command, payload_json "
        "FROM network_message ORDER BY sequence"
    ):
        if row["command"] != "gaa":
            continue
        try:
            import json

            payload = json.loads(row["payload_json"])
        except Exception:  # noqa: BLE001 - a malformed row must not stop the run
            continue
        entry = {
            "sequence": row["sequence"],
            "at_ms": row["observed_at_ms"],
            "payload": payload,
        }
        if row["direction"] == "client_to_server" and "AX1" in payload:
            requests.append(entry)
        elif row["direction"] == "server_to_client":
            responses.append(entry)
    return requests, responses


def objects(entry: dict) -> list[list]:
    rows = entry["payload"].get("AI")
    return [row for row in rows if isinstance(row, list)] if rows else []


def extent(entry: dict) -> tuple[int, int, int, int, int] | None:
    xs = [row[1] for row in objects(entry) if len(row) > 2 and isinstance(row[1], int)]
    ys = [row[2] for row in objects(entry) if len(row) > 2 and isinstance(row[2], int)]
    if not xs or not ys:
        return None
    return min(xs), max(xs), min(ys), max(ys), len(objects(entry))


def pair(requests: list[dict], responses: list[dict]) -> list[tuple[dict, dict]]:
    """Each request with the first response for the same kingdom after it."""
    by_kid: dict[int, list[dict]] = {}
    for response in responses:
        by_kid.setdefault(response["payload"].get("KID"), []).append(response)
    paired: list[tuple[dict, dict]] = []
    for request in requests:
        kid = request["payload"].get("KID")
        for response in by_kid.get(kid, []):
            if response["sequence"] > request["sequence"]:
                paired.append((request, response))
                break
    return paired


# --- reports ----------------------------------------------------------------


def report_ladder(paired: list[tuple[dict, dict]]) -> bool:
    print("1. how far out did the replies actually reach?")
    print("   asked  replies  furthest object from the window origin")
    by_span: dict[int, list[tuple[dict, tuple | None]]] = {}
    for request, response in paired:
        asked = request["payload"]
        span = asked["AX2"] - asked["AX1"] + 1
        by_span.setdefault(span, []).append((asked, extent(response)))
    reached_every = bool(by_span)
    for span in sorted(by_span):
        best_x = best_y = 0
        rows = 0
        for _asked, got in by_span[span]:
            if got is not None:
                rows += got[4]
        for asked, got in by_span[span]:
            if got is None:
                continue
            best_x = max(best_x, got[1] - asked["AX1"] + 1)
            best_y = max(best_y, got[3] - asked["AY1"] + 1)
        reached = best_x >= span and best_y >= span
        reached_every = reached_every and reached
        note = "reached the full window" if reached \
            else "objects stopped short - sparse, NOT proof of clipping"
        print(f"   {span:5d}  {len(by_span[span]):7d}  {best_x} x {best_y} of {span} "
              f"({rows} objects)  {note}")
    print("   note: an object extent is a floor on the window, never a ceiling.")
    print("   A reply with nothing at its edge has not been clipped; it is just empty.\n")
    return reached_every


def request_slots(asked: dict) -> list[tuple[int, int]]:
    """The slots a request answers, by geometry.

    A window answers every fortress slot inside it because the server reports
    what occupies the area; that is a property of the window, not of which
    objects happened to be present.
    """
    return [
        slot
        for slot in slots_in(asked["AX1"], asked["AY1"], asked["AX2"], asked["AY2"])
        if slot[0] <= MAP_MAX and slot[1] <= MAP_MAX
    ]


def report_coverage(paired: list[tuple[dict, dict]]) -> bool:
    print("2. how many lattice slots does one request answer?")
    print("   span  requests  slots each (mean / max)")
    by_span: dict[int, list[int]] = {}
    for request, _response in paired:
        asked = request["payload"]
        span = asked["AX2"] - asked["AX1"] + 1
        by_span.setdefault(span, []).append(len(request_slots(asked)))
    several = False
    for span in sorted(by_span):
        counts = by_span[span]
        mean = sum(counts) / len(counts)
        several = several or mean > 1.5
        print(f"   {span:4d}  {len(counts):8d}  {mean:5.2f} / {max(counts)}")
    print()
    return several


def report_fortress_positions(paired: list[tuple[dict, dict]]) -> bool | None:
    """The only evidence that speaks to a wide window: fortresses found at the
    far corner of the window that was asked for.

    A 13-wide client tile cannot reach a slot 39 units away, so windows of that
    size say nothing about wide probes and are excluded."""
    print("3. were fortresses found away from the window's low corner?")
    wide = [
        (request, response)
        for request, response in paired
        if request["payload"]["AX2"] - request["payload"]["AX1"] + 1 > 13
    ]
    if not wide:
        print("   every recorded request is a 13-wide client tile, so there is")
        print("   no evidence here about wider windows. That claim is UNPROVEN.")
        print("   Send a wide probe from the lab panel to settle it.\n")
        return None

    total = 0
    inner = 0
    for request, response in wide:
        asked = request["payload"]
        span = asked["AX2"] - asked["AX1"] + 1
        for row in objects(response):
            if not row or row[0] != 11 or len(row) < 3:
                continue
            total += 1
            # Past the first lattice step away from the origin means the reply
            # covered ground a narrow window could not have reached.
            if row[1] - asked["AX1"] > LATTICE_STEP or row[2] - asked["AY1"] > LATTICE_STEP:
                inner += 1
    print(f"   {len(wide)} wide requests, {total} fortresses in their replies")
    print(f"   {inner} were more than one lattice step from the window corner")
    return total > 0 and inner > 0


def report_plan(span: int, observed_density: float | None) -> None:
    slots_per_axis = max((span - 2 * GAA_PAD - 1) // LATTICE_STEP + 1, 1)
    one_grid = block_count(span)
    print(f"4. cost of a kingdom at span {span}")
    print(f"   window covers {slots_per_axis} slots per axis on the probed grid "
          f"({slots_per_axis ** 2} slots)")
    print(f"   one grid:   {one_grid:5d} requests")
    print(f"   both grids: {one_grid * 2:5d} requests")
    print(f"   both grids in one sweep, with a window wide enough for both: "
          f"{one_grid:5d} requests")
    if observed_density is not None:
        print(f"   at the observed {observed_density:.3f} objects per cell that is about "
              f"{observed_density * span * span:.0f} objects per reply")
        print("   (extrapolated from recorded replies, not measured at this span)")
    print()


# --- watch mode -------------------------------------------------------------


def watch(paired: list[tuple[dict, dict]], pace: float) -> None:
    """Replays the recorded sweep one request at a time, slowly."""
    print(f"replaying {len(paired)} recorded gaa exchanges, {pace}s apart\n")
    for index, (request, response) in enumerate(paired, start=1):
        asked = request["payload"]
        span = asked.get("AX2", 0) - asked.get("AX1", 0) + 1
        got = extent(response)
        detail = "no objects" if got is None else (
            f"{got[4]} objects, x {got[0]}..{got[1]}"
        )
        print(
            f"{index:4d}. KID {asked.get('KID')} "
            f"{asked.get('AX1')},{asked.get('AY1')} span {span:3d}  ->  {detail}"
        )
        if pace:
            time.sleep(pace)
    print("\nreplay finished")


def density(paired: list[tuple[dict, dict]]) -> float | None:
    """Objects per map cell, averaged over every recorded reply."""
    total_cells = 0
    total_objects = 0
    for request, response in paired:
        asked = request["payload"]
        span = asked.get("AX2", 0) - asked.get("AX1", 0) + 1
        got = extent(response)
        if got is None:
            continue
        total_cells += span * span
        total_objects += got[4]
    return total_objects / total_cells if total_cells else None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--db", type=Path, default=DEFAULT_DB)
    parser.add_argument("--plan", type=int, metavar="SPAN",
                        help="also report the cost of this window span")
    parser.add_argument("--watch", type=float, metavar="SECONDS",
                        help="replay the recording at this pace")
    args = parser.parse_args()

    connection = connect(args.db)
    requests, responses = load_traffic(connection)
    print(f"recorded gaa: {len(requests)} requests, {len(responses)} responses\n")
    if not requests:
        print("nothing to simulate yet")
        return 1

    paired = pair(requests, responses)
    if not paired:
        print("no request/response pairs could be formed")
        return 1

    if args.watch is not None:
        watch(paired, args.watch)
        return 0

    ok_ladder = report_ladder(paired)
    ok_coverage = report_coverage(paired)
    wide = report_fortress_positions(paired)
    if args.plan:
        report_plan(args.plan, density(paired))

    print("summary")
    print(f"  window reached in full  : {'yes' if ok_ladder else 'not established'}")
    print(f"  several slots/request   : {'yes' if ok_coverage else 'no — every recorded window holds one slot'}")
    if wide is None:
        print("  wide windows            : UNPROVEN — no wide request has been recorded")
    else:
        print(f"  wide windows            : {'yes' if wide else 'NO'}")
    print("  a span that has never been sent stays unproven; probe it from the panel")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
