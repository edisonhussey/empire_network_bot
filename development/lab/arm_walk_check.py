#!/usr/bin/env python3
"""Replay the four-arm discovery walk against real fortress rows.

Offline only: it reads a database backup, never the game socket. The point is to
check the rectangle the arms measure against the fortresses that actually exist,
rather than against a band invented for the unit tests.

    python3 arm_walk_check.py <backup.sqlite3>

The algorithm is a direct port of `FortressDiscovery` in
`rust_distributable/crates/empire-daemon/src/direct.rs`, so the two can be
compared line by line. If they disagree, this script is wrong.
"""

import sqlite3
import sys

LATTICE_STEP = 39
LATTICE_OFFSETS = (9, 29)
SLOTS_PER_FAMILY_PER_AXIS = 3
BLOCK_SPAN = LATTICE_STEP * 2 + 20
BLOCK_STEP = BLOCK_SPAN + 19
SWEEP_ALIGNMENT = 9
GAA_PAD = 1

ARM_STEPS = ((1, 0), (-1, 0), (0, 1), (0, -1))
ARM_EMPTY_LIMIT = 2
ARM_MAX_STEPS = 32

OUTER_MAP_MAX_COORD = 1285


def block_origin(point):
    """The origin of the block whose window spans `point`."""
    return tuple(p - (p - SWEEP_ALIGNMENT) % BLOCK_STEP for p in point)


def window(origin):
    """The inclusive coordinate range a gaa request for this block asks about."""
    return (
        origin[0] - GAA_PAD,
        origin[1] - GAA_PAD,
        origin[0] + BLOCK_SPAN + GAA_PAD,
        origin[1] + BLOCK_SPAN + GAA_PAD,
    )


def holds(fortresses, origin):
    left, top, right, bottom = window(origin)
    return any(
        left <= x <= right and top <= y <= bottom for x, y in fortresses
    )


def walk(fortresses, base):
    """Four arms out from `base`, each stopping after two empty windows."""
    reached = []
    probes = 0

    # The base first; a window spanning the castle is always full.
    probes += 1
    if holds(fortresses, base):
        reached.append(base)

    for dx, dy in ARM_STEPS:
        origin = base
        empties = 0
        for _ in range(ARM_MAX_STEPS):
            # Advance first: an empty window must still move the arm, or it
            # re-asks about the same hole and can never step past it.
            origin = (origin[0] + dx * BLOCK_STEP, origin[1] + dy * BLOCK_STEP)
            probes += 1
            if holds(fortresses, origin):
                reached.append(origin)
                empties = 0
            else:
                empties += 1
                if empties >= ARM_EMPTY_LIMIT:
                    break
    return reached, probes


def bounds_covering(origins):
    xs = [o[0] for o in origins]
    ys = [o[1] for o in origins]
    return (min(xs), min(ys), max(xs) + BLOCK_SPAN, max(ys) + BLOCK_SPAN)


def block_count(bounds):
    """Blocks in the rectangle: derived from ORIGINS, not from the far edge.

    `right` is the last origin plus `BLOCK_SPAN`, so it is an edge, not an
    origin. Treating it as one overcounts by a whole row and column - which this
    script did until the numbers were checked against the Rust test, which
    asserts 56 for the Sands bounds this prints 72 for.
    """
    def axis(low, high):
        last_origin = high - BLOCK_SPAN
        return (last_origin - low) // BLOCK_STEP + 1

    return axis(bounds[0], bounds[2]) * axis(bounds[1], bounds[3])


def flat_blocks():
    """Blocks per axis for the old hardcoded sweep, and its total."""
    upper = OUTER_MAP_MAX_COORD - (OUTER_MAP_MAX_COORD - SWEEP_ALIGNMENT) % BLOCK_STEP
    per_axis = (upper - SWEEP_ALIGNMENT) // BLOCK_STEP + 1
    return per_axis, per_axis * per_axis


def main():
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
    rows = db.execute(
        "SELECT kingdom_id, x, y FROM fortress_target ORDER BY kingdom_id, x, y"
    ).fetchall()
    if not rows:
        print("no fortress_target rows in that database")
        return 1

    by_kingdom = {}
    for kingdom_id, x, y in rows:
        by_kingdom.setdefault(kingdom_id, []).append((x, y))

    per_axis, flat = flat_blocks()
    print(f"flat sweep: {per_axis} x {per_axis} = {flat} blocks\n")

    for kingdom_id in sorted(by_kingdom):
        fortresses = by_kingdom[kingdom_id]
        castles = db.execute(
            "SELECT x, y FROM owned_castle WHERE kingdom_id = ?", (kingdom_id,)
        ).fetchall()
        if not castles:
            print(f"kingdom {kingdom_id}: {len(fortresses)} fortresses, no castle row")
            continue
        base = block_origin(castles[0])
        reached, probes = walk(fortresses, base)
        if not reached:
            print(f"kingdom {kingdom_id}: the walk measured nothing")
            continue
        bounds = bounds_covering(reached)
        fill = block_count(bounds)
        missed = [
            (x, y)
            for x, y in fortresses
            if x < bounds[0] or x > bounds[2] or y < bounds[1] or y > bounds[3]
        ]
        print(
            f"kingdom {kingdom_id}: castle {castles[0]} base {base}\n"
            f"  {len(fortresses)} fortresses, bounds {bounds}\n"
            f"  fill {fill} blocks (flat {flat}), {probes} discovery probes, "
            f"{probes + fill} total\n"
            f"  MISSED {len(missed)}"
        )
        for x, y in missed[:10]:
            print(f"    outside: ({x},{y})")
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
