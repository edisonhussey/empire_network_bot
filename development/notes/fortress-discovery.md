# Fortress discovery

How the bot finds fortresses, what is proven about it, and what is still
assumed. Every "proven" line names the evidence so it can be re-checked.

## 1. Fortresses sit on a lattice

```
x ≡ y (mod 39)   and   x mod 39 ∈ {9, 29}
```

**One lattice, not two.** That is a single congruence: a point is a slot when
`x ≡ y (mod 39)` and the shared residue is `9` or `29`. Walking any diagonal,
consecutive slots are **19 or 20 apart, alternating**:

```
coords   9  29  48  68  87  107  126  146  165  185  204
gaps       20  19  20  19   20   19   20   19   20   19
residue    9  29   9  29    9   29    9   29    9   29    9
```

So the one lattice falls into two residue families, interleaved. `LATTICE_OFFSETS
= [9, 29]` in `fortress.rs` names those two families; it is not two independent
grids. Earlier wording in these notes said "two grids", which is wrong and fed a
wrong cost model (see §6).

**Evidence.** 19 original captured type-11 rows, then re-measured on 126
fortresses observed live in one run (40 / 41 / 45 across Sands / Ice / Fire):
every single row had `(x − y) % 39 = 0` and `x % 39 ∈ {9, 29}`. Zero exceptions.
`crates/empire-core/src/fortress.rs` carries the captured positions as a test
fixture and asserts the rule.

**Consequence.** The coordinates worth visiting are enumerable, so discovery is
a sweep over a known finite set rather than a search. A probe must be anchored
*on* a slot; the first implementation stepped a flat ±17, which lands between
slots and could never find anything, however long it ran.

## 2. The map is finite

The last usable lattice coordinate is **1277**; the map rectangle is bounded at
1285.

**Evidence.** Live boundary probes returned normal terrain through windows
rooted at 1280, partial terrain at x=1286, and nothing at y=1286 or 1290. A
recorded reply to a window at `1276,808 → 1317,849` came back spanning only
`1276..1286` — the world cutting the reply, not the request.

## 3. The sweep

One cursor per kingdom. `fortress_scan_state` holds the bounds and the cursor;
`FortressScanCursor` walks it row-major in 78-unit steps, and each step's window
spans both residue families.

- 33 slots per axis per family → 17 blocks per axis → **289 blocks per kingdom**
  (was 578 when the two families were swept separately; V16 clears the cursors
  that change invalidated).
- The cursor only advances when the server has answered, so a lost reply costs a
  repeat rather than silently skipping four slots. This split (`block()` reads,
  `advance()` moves) is deliberate.
- Bounds are `Bounds::outer_kingdom()` aligned to the lattice, so the sweep
  starts on the lattice and ends at the far corner.

**Proven.** `the_sweeps_cover_every_slot_in_the_kingdom` — every one of the 2,178
slots falls inside some block, so no fortress can be missed.
`the_sweep_exhausts_the_whole_kingdom` — all 578 blocks are visited and the
cursor reaches `(1277, 1277)`.

**Not proven.** That the walk is *ordered usefully*. It is row-major from the
map's low corner, so fortresses beside your castle appear partway through rather
than first. Completeness is unaffected.

## 4. One request, five slots

A probe asks for `block.window()`: `x − 1 … x + 39 + 1`, i.e. **42 wide**, which
contains both of the block's extreme slots with one unit of clearance — and, per
§5, unavoidably the interior slot of the other residue family as well.

**Proven (from recorded traffic).** Eleven recorded requests asking for
`AX2 − AX1 = 41` each returned a reply spanning the full 42 units — e.g.
`asked 262,886 → 303,927; returned 262..303 × 886..927, 370 objects`. The reply
is **not** clipped to a fixed lower-bound box.

**Proven (from outcomes).** Fortresses were found at all four positions inside
their blocks, e.g. for a completed grid: 81 / 81 / 81 / 81 across the four
residue combinations. A clipped reply could only ever have produced the
low-corner one.

## 5. The window width — measured live

**A 42-wide window is honoured in full.** Verified on the live Ventrilo account,
Sands, map view, 2026-10-05, through the lab panel:

```
sent      %xt%EmpireEx_21%gaa%1%{"KID":1,"AX1":632,"AY1":632,"AX2":673,"AY2":673}%
accepted  202 {"status":"queued"}
reply     x 632..673 (42)   y 632..673 (42)   370 objects
          kinds {1:2, 2:65, 10:224, 11:5, 12:71, 28:2, 31:1}
```

In the same second the same socket returned two 13-wide client tiles
(`608..620`, `621..633`), each exactly 13 wide. **The reply size tracks the
request, not the client's tile**, so the bounds are a real rectangle and not a
viewport hint.

**Five fortresses from one request.** The window contains exactly five lattice
slots — `(633,633) (633,672) (653,653) (672,633) (672,672)` — and the reply
carried exactly five type-11 rows. All five are now in `fortress_target`, all
level 45, each with its own cooldown. Every slot in that window held a fortress.

That settles the question this note opened with: one request really does answer
several slots, and the 2×2 block model understates what is available.

### The exact-bounds test — 4 fortresses, zero padding

Worked out from the database first, then probed, so the reply confirms the model
rather than producing a number to rationalise afterwards.

The tightest rectangle holding **exactly four** known fortresses is **20 × 59**,
and it is not a square — it takes one coordinate per family on `x` and two per
family on `y`:

```
box      263,224 -> 282,282
holds    (263,224) (263,263)   <- residue 29, x = 263
         (282,243) (282,282)   <- residue 9,  x = 282
slots inside the box: exactly these four, nothing else

why exactly four:
  x range 263..282 (20 wide) holds 1 coordinate per family  (263 -> 29, 282 -> 9)
  y range 224..282 (59 wide) holds 2 coordinates per family (224/263 -> 29)
                                                            (243/282 -> 9)
  slots = (x coords of a family) x (y coords of the same family), summed:
          1x2 for residue 29  +  1x2 for residue 9  =  4
```

```
sent A   {"KID":1,"AX1":263,"AY1":224,"AX2":282,"AY2":282}      span 20 x 59
reply    x 263..282  y 224..282   253 objects, 4 type-11
returned (263,224) (263,263) (282,243) (282,282)   missing none, extra none

sent B   {"KID":1,"AX1":262,"AY1":223,"AX2":283,"AY2":283}      span 22 x 61
reply    x 262..283  y 223..283   294 objects, 4 type-11
returned the same four                              missing none, extra none
```

Three things fall out of this:

1. **Four fortresses in one request, on prediction.**
2. **The bounds are inclusive.** Fortresses sitting exactly on the `AX1`/`AY1`
   and `AX2`/`AY2` lines came back, and the reply extent matched the asked box
   exactly — so the server clips to the request rather than returning more.
3. **`GAA_PAD` is not needed.** One unit of padding added no fortresses and no
   information. It is kept at `1` as cheap insurance against an off-by-one, but
   `0` is proven safe if a smaller window is ever wanted.

**A square block always covers five slots, not four.** The centre of a 2×2 block
is a slot of the *other* residue family, and it lands inside any square that
contains the block.

Measured over the 806 known Sands fortresses:

```
residue-9 blocks whose low corner is held:  102
  ... with all four corners held:            90
  ... and the centre slot held too:          90   <- every single one
held slots: residue 9 = 384, residue 29 = 422 (of 1089 each)
```

So the centre is not empty. The *exact* midpoint of four same-family slots is at
`+19.5`, which is not an integer and cannot hold a fortress — that part of the
intuition is right — but it is not the centre of the window. The gaps alternate
20/19, so the interior slot sits `+20` from one corner and `−19` from the other,
and in dense ground it is occupied as reliably as the corners are.

Live confirmation of the residue split, straight from `fortress_target`:

```
kingdom 1: 806 fortresses   x mod 39 -> {9: 384, 29: 422}   (x-y)%39==0: 806/806
kingdom 2:  41 fortresses   x mod 39 -> {9:  16, 29:  25}   (x-y)%39==0:  41/41
kingdom 3: 638 fortresses   x mod 39 -> {9: 324, 29: 314}   (x-y)%39==0: 638/638
```

## 6. The efficient unit is a wide window covering both residue families

Because the two families interleave at 19/20 spacing, one honouring window can
cover slots of **both** of them. Widen it to two slots per axis per family and it
answers eight slots, so one sweep replaces two — the whole cost model above is
an artifact of treating the families as separate grids. Verified live, predicted
from the database first:

```
window   633,633 -> 692,692   span 60 x 60
reply    x 633..692 (60)  y 633..692   720 objects, 8 type-11
returned (633,633) (633,672) (653,653) (653,692)
         (672,633) (672,672) (692,653) (692,692)
predicted 8 | got 8 | missing none | extra none
```

The full ladder, at `n` slots per axis per family, window span `39n − 18` (the
server counts cells inclusively, so `692 − 633 + 1 = 60`), stepping `39n`:

| span | slots per request | windows per axis | requests per kingdom | vs today |
|---|---|---|---|---|
| 21 | 2 | 33 | 1,089 | 0.5× |
| 42 (current) | 5 | 17 | 578 | 1.0× |
| **60** | **8** | **17** | **289** | **2.0×** |
| 99 | 18 | 11 | 121 | 4.8× |
| 138 | 32 | 9 | 81 | 7.1× |
| 177 | 50 | 7 | 49 | 11.8× |
| 216 | 72 | 6 | 36 | 16.1× |

Span 60 halves the cost at no extra risk: the same 17 windows per axis, just
covering both families in each one. Above 60 the reply grows — ~8,000 objects at
span 216, at the observed density of ~0.2 objects per cell — and that limit is
unmeasured. 60 is measured, and the reply fitted it exactly with 720 objects.

**Implemented in 0.1.33**, parameterised as `SLOTS_PER_FAMILY_PER_AXIS` so the
window is one constant to widen (`BLOCK_SPAN` and `BLOCK_STEP` follow). 289
requests per kingdom, and the de-duplication is now proved by test rather than by
the enumeration above: `one_sweep_covers_every_slot_exactly_once` asserts every
one of the 2,178 slots is answered exactly once.

**Tempo.** `fortress_probe_delay_seconds` is now ~0.2 s — nine probes in ten take
`uniform(0.18, 0.22)` and the tenth takes `uniform(0.28, 0.45)` — so a kingdom is
about a minute instead of eleven. That is a much denser burst than the old
1.05–1.25 s (roughly 4.6 requests a second against 0.87); it was chosen
deliberately, but it is the one number here that has not been watched on the live
server for a full kingdom yet.

### What the live server actually sustains — measured 2026-10-05

`development/lab/pace_probe.py` fires a short burst of real window requests
through `/v1/injections` and pairs each one with its reply by sequence number.
Nineteen probes across three runs, all answered, **no drops**:

| run | spacing | replies | reply latency (queued → answered) |
|---|---|---|---|
| 10 probes | 0.22–0.42 s | 10/10 | **1.46 s, growing to 7.67 s** |
| 5 probes | 0.6–0.8 s | 5/5 | 0.85 s, creeping to 1.72 s |
| 4 probes | 1.4–1.6 s | 4/4 | 0.86–1.04 s, flat |

The middle and bottom rows are the informative ones. Sent slower than the server
can answer, latency is a **flat ~0.85–0.9 s**. Sent faster, latency climbs
linearly — that is a queue forming, not a refusal.

**So the server's service time for one `gaa` is about 0.9 s, and its ceiling is
roughly 1.1–1.2 answers per second.** Firing at 0.22 s does not get answers
faster, it just builds a backlog: in the top row the tenth reply landed 7.7 s
after its request.

**What that means for the walk.** The runner keeps exactly one probe in flight
and waits for the reply, so its real tempo is `service time + configured delay`:

```
289 probes × (0.9 s + 0.22 s) ≈ 5.4 min per kingdom
289 probes × (0.9 s + 1.15 s) ≈ 9.9 min      (the old tempo, after span-60)
578 probes × (0.9 s + 1.15 s) ≈ 19.8 min     (the old tempo, before span-60)
```

So the two changes multiply: span-60 halves the probes, and the shorter delay
removes most of the added wait. **The floor is ~4.3 min** — 289 × 0.9 s — and no
pacing choice can go below it, because the binding constraint is the server, not
us.

**Caveat, stated plainly.** This probe fires without waiting, so it measures the
server's throughput ceiling. The runner serialises, so it never queues and its
numbers should be the `service + delay` line above rather than the backlog. That
line has not been observed end-to-end; doing so needs the new build running a
real walk.

### Pipelining does not buy speed — and the reply cannot be attributed

The obvious next idea is to stop waiting: keep N windows in flight, process
replies as they land, and so hide the round trip. **The 10-probe run already did
that**, and it is why the ceiling was measurable at all. Sending 10 unacked at
0.22 s drained at **~1.14 answers/s** — the same as one-at-a-time. Keeping 50 in
flight at the same time would not change it, because ~0.9 s is a *throughput*
figure, not a round trip. The server sets the pace; the delay only adds to it.

Worse, out-of-order processing is blocked by a protocol fact: **a `gaa` reply
does not echo the window that was asked for.** That is the recorded defect **D7**,
and it is the whole reason the runner sends one probe and waits —
`block()` reads, `advance()` moves, and the cursor only moves once a reply has
been seen. With two windows in flight, a reply cannot be attributed to a request,
so a lost one would silently skip ground rather than cost a repeat.

A bounded in-memory buffer of recent replies is still worth having, but as
*reliability* (dedupe, tolerate a late reply, survive a reconnect) rather than as
a speed play. Speed has to come from fewer, wider requests: the span-60 tiling is
the lever, and it is already in.

### The window ceiling is between 99 and 138 — 138 is refused

`pace_probe.py --ladder` sends one window per width, widely spaced and in
scrambled order, so latency can be read per width:

```
width                latency s      objects   verdict
1 family   23 cells  0.244 / 0.269     101    honoured
2 families 62 cells  0.528..1.035      777    honoured
3 families 101 cells 0.122..0.476     2063    honoured
4 families 138 cells 0.117 / 0.118       0    REFUSED
```

The 138-cell replies came back with a **completely empty payload `{}`**, where a
real reply carries `['AI','KID','OI','uap']`. That is the same tell as a rejected
`adi` — **empty payload means refused**, not "no objects here". And it cannot be
empty ground: the window `476,476..615,615` contains **32 known fortresses**, and
the other width-4 window covers 8 known ones.

So span 99 is viable and span 138 is not. The ladder is the only reason we know —
138 had been assumed fine because 60 was.

**Latency is not a function of width, and not of reply size.** The widest window
was the *fastest* (0.12 s) and the narrowest was not the slowest. The 62-cell
window took ~0.97 s in one area and ~0.58 s in another with the same object count.
So there is no single "service time": per-request latency varies with the request
and the ground, somewhere between 0.12 s and 1.0 s, and **the earlier "~0.9 s per
`gaa`, ~1.05 answers/second ceiling" was over-generalised from one area.** It is
retracted as a general figure; it was real for those windows and is not a
constant to plan against.

**Replies come back in order.** Both ladder runs sent the four widths scrambled
and the replies arrived in exactly the send order — FIFO, not sorted by area or
size. So the server is not handing back unordered replies at this depth, and
pipelining does not need to solve reordering before it solves attribution.


### Completeness at a fast tempo — confirmed, then the width was raised

Ten span-60 windows across a dense strip of Sands, fired at a 0.29 s mean gap,
**predicted from `fortress_target` before being sent**:

```
window     expected  returned  missing  extra
  (all ten)      8         8        0      0      <- 80/80 fortresses
sent 10 in 2.74 s | replied 10 | reply gap mean 0.95 s | missing 0
```

Every window returned exactly the eight the database knew about — nothing
dropped, nothing extra, nothing attributed to the wrong window. So a fast tempo
is safe *and* complete.

**The shipped setting is now three families: a 100-cell window, 121 requests per
kingdom, and a 0.7 s ± random delay.** That lands the walk in the 1–2 minute band
the progress bar is sized against, which is why the width went up:

| families | window cells | probes/kingdom | delay | walk |
|---|---|---|---|---|
| 2 | 62 | 289 | 0.25 s | ~4.6 min |
| **3** | **100** | **121** | **0.7 s** | **~1–2 min** |
| 4 | 138 | 81 | — | **refused** |

Three is the widest the server honours, so this is the ceiling rather than a
preference. The count is known exactly (121), so the progress bar is a real
fraction and not an estimate. Widening further is one constant
(`SLOTS_PER_FAMILY_PER_AXIS`), and `development/lab/pace_probe.py --ladder` is
what says whether a wider step is honoured.

### The current sweep probes 545 slots twice

Easy to miss, because the two sweeps look independent. They are not: the centre
of a residue-9 block is a residue-29 slot, which that family's own sweep also
visits. Counted over the whole map:

```
span-42, two sweeps:   578 windows   2178 slots
   in 1 window  1633 slots
   in 2 windows  545 slots        <- 20% of the work is redundant

span-60, one sweep:    289 windows   2178 slots
   in 1 window  2178 slots        <- every slot exactly once, none missed
```

So "don't double count" is a real defect in the current model, not a
hypothetical: 545 slots are requested twice. The span-60 tiling is provably
exact — verified by enumeration above, and by the live probe in the table.

### The walk is one-time work and must not block a session

Fortress coordinates never change, so the sweep is a discovery cost paid once
per account and resumed if interrupted — never repeated. `fortress_scan_state`
persists the cursor, and `Store::outstanding_fortress_sweeps` answers which
sweeps still have blocks left. When it answers empty the runner skips the phase
entirely: no settle delay, no per-kingdom pause, and the session reports ready
instead of claiming to map coordinates it is not going to request.

Live example (Ventrilo, 2026-10-05): four of the six possible sweeps were at
`289/289` with the cursor past the bottom edge, so a session before this change
still spent the 5 s settle plus two 3.4–4.8 s kingdom-switch pauses flashing
`discovering_fortresses` for work that was already finished.

**Only the running bot's kingdoms are walked, and only when it runs.** The walk
exists to feed fortress tasks, so it waits until such a task is in the bot that
is *actually started*: `Store::active_fortress_kingdoms()` joins `account_mode
(running = 1)` → `automation_mode_task` → `task_subscription(target_kind =
'fortress')`. A robber-baron bot, or a fortress task sitting in a bot nobody
started, costs nothing. The plan's enabled kingdoms remain the outer bound.

The plan is rebuilt on a 2 s timer rather than fixed at connect, because the bot
is chosen *after* the socket opens. That is what makes "only when it needs it"
work: connect alone walks nothing, pressing Start on a fortress bot walks that
kingdom, and stopping puts it back to sleep. Automation gates on "no active block
and the sweep list is exhausted", so an account with nothing to walk starts
immediately instead of paying the old 5 s settle and per-kingdom pauses.

**Consequences for the code.** The 2×2 block model in `fortress.rs` is a safe
sub-unit of this, not the optimum. Moving to it means one cursor per kingdom
instead of one per residue family, the window from 42 to 60, `BLOCK_STEP`
unchanged at 78, and roughly half the requests. Not yet implemented.

**How to settle the wider spans.** `development/lab` → the window ladder. One
request at a time, two per two seconds, no rebuild. The panel reports the asked
span against the returned extent, computed from the stored reply.

## 7. Things that were tried and were wrong

Kept because the reasoning is a trap worth recognising.

| attempt | why it failed |
|---|---|
| step a flat ±17 from the castle | lands between slots; found nothing. All 9 frontier rows it created were off-lattice. |
| prune the queue to the box around fortresses found so far | the deletions are permanent, so each kingdom collapsed to the fortresses within ±78 of its castle — 40/41/45 — while the rest of the kingdom stayed unqueued. Looked like a working scan. |
| order the walk by distance from the castle | lost when the per-coordinate queue was replaced by a cursor. Not a bug, but the ordering is gone. |

## 8. Live counts worth remembering

- Occupancy inside a fortress cluster is high — 40 fortresses across the 50 slots
  of a 156×156 box — but it is not uniform across the map, so slots must be
  probed and never inferred from neighbours.
- A 13-wide client tile contains **exactly one** lattice slot (measured: mean
  1.00, max 1 over 44 recorded tiles). That is why the radius map scan is a poor
  way to find fortresses: it would need ~2,178 requests per kingdom.

## 9. The extent is measured, not assumed (0.1.35)

Everything above assumes the walk covers a fixed rectangle derived from
`OUTER_MAP_MAX_COORD = 1285`. That is one server's map. A kingdom on a smaller
map pays for ground that is not there, a larger one is silently truncated, and
the constant is a guess either way. From 0.1.35 the rectangle is measured.

**The walk.** Take the block over the kingdom's own castle — the RBC scan
origins already hold that coordinate — as the base. Its window spans the castle,
so the first probe is always a full one. Then walk four arms from the base,
right, left, down and up, one window at a time, and stop each arm after
**two consecutive empty windows**. The rectangle those four stops bound is the
kingdom's extent; the existing row-major cursor then fills it.

Simulated against the backed-up Sands set (806 fortresses around a castle at
593,613): base block 477,594 holds 18; the arms run +5/−3/+4/−5 blocks and
settle on 7×8 = 56 blocks covering x 242..1044, y 125..1044, with **0 of the 806
outside it**. That is 56 requests where the flat sweep is 121.

**Verified against the real rows in the pre-wipe backup**
(`development/lab/arm_walk_check.py <backup.sqlite3>`, offline, reads the DB
read-only). All three kingdoms, **0 missed in every one**:

| kingdom | castle | base | fortresses | measured bounds | fill | probes | total | flat |
|---|---|---|---|---|---|---|---|---|
| 1 Sands | 593,613 | 477,594 | 806 | 243,126 → 1043,1043 | 56 | 22 | **78** | 121 |
| 2 Ice | 654,696 | 594,594 | 41 | 477,594 → 809,809 | 6 | 12 | **18** | 121 |
| 3 Fire | 688,582 | 594,477 | 638 | 243,243 → 1043,1043 | 49 | 21 | **70** | 121 |

166 requests across all three kingdoms against the flat sweep's 363. Note Ice at
18: a small kingdom is cheap now, which is exactly what a hardcoded rectangle
could never do. The script's own block count was wrong on the first run (it read
the far edge as an origin and reported 72 for Sands); the Rust test asserting 56
is the authority, and it is why cross-checking the two matters.

**Why the guard is two and not one.** An empty window inside a band is normal —
a block with no fortress in it between two that have some. Ending an arm on the
first empty one truncates the rectangle at the first hole and every fortress
past it is missed. The second empty window is what makes it a stop rather than a
gap. `a_gap_inside_the_band_does_not_end_an_arm` pins this.

**The arm must advance on an empty window.** The first version of the guard
incremented its empty-counter without moving the cursor, so it re-probed the
same block twice and "confirmed" the same hole — it could never cross a gap,
which is the whole thing the guard exists for. Caught by the test, not by
reading it.

**A refusal is not a full window.** A refused request comes back as `{}` (see
§5), which is indistinguishable from ground with nothing on it. It is counted as
empty, so a refusal *shortens* the rectangle. That direction is deliberate: it
costs coverage, which is visible as a missed fortress, whereas the other choice
extends the rectangle over ground nothing ever looked at and calls it finished.

**Where it lives.** `FortressDiscovery` in `direct.rs` holds the state machine;
`block_origin` and `bounds_covering` in `fortress.rs` do the geometry; the
measured rectangle is persisted by `Store::set_fortress_scan_bounds` and read
back by `Store::fortress_scan_bounds`, which is also how "never walked" is told
apart from "walked and finished" — `next_fortress_block` answers `None` for
both. V17 clears every pre-0.1.35 row, because those carry the old rectangle.

