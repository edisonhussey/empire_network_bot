# Target selection

Which Robber Baron castle (tower) to attack next, in the four kingdoms (green, ice,
fire, sand). Fortresses are selected separately and are not covered here. The code is
`empire-core/src/targeting.rs`; the store feeds it through
`reserve_rbc_target_with`.

Selection is pure arithmetic over towers the database already marks ready (cooldown
over, not leased, server says ready, level in range). It costs **no network
requests**: the only packets are the ones that attack the chosen tower. Readiness
itself comes from the database and the server's own cooldowns, never from polling; a
tower with no recent record in its cooldown window is assumed ready.

## `advanced`: a moving spotlight

A tower's weight is

```text
w = 1 / ( (d_home + c) ^ p * (d_spotlight + c) )
```

* `d_home`: distance from the task's source castle (the kingdom's main castle).
* `d_spotlight`: distance from a point that follows productive ground.
* `c = 5` tiles keeps a tower on top of the castle or spotlight from taking an
  unbounded weight. `p = 1` is the formula as specified; see "Tuning" before changing it.

One pass chooses with **weighted reservoir sampling**: keep a running weight total,
and let each eligible tower replace the current pick with probability
`weight / total`. That is an exact weighted draw in O(n) time and O(1) extra memory.
The database rows are streamed past it one at a time, so nothing proportional to the
number of towers is allocated, and there is no sorting or normalising pass.

The same pass does two more jobs:

* counts eligible towers within `local_radius` (12 tiles) of the spotlight;
* keeps a second reservoir weighted by home distance alone.

While any tower is left near the spotlight, it stays. When none is, the spotlight
relocates to the second reservoir's pick (home-weighted, spotlight-independent).

### Momentum

After a tower is **accepted** (its attack acknowledged, not merely chosen), the
spotlight moves by velocity, not acceleration:

```text
v' = mu * v + (1 - mu) * (selected - previous_selected)
p' = selected + gamma * v'
```

with `mu = 0.7`, `gamma = 2`, each component clamped to +-8 tiles. With no previous
selection the random starting velocity is kept. A relocation puts the spotlight on the
chosen tower with a fresh small random velocity (1 to 3 tiles). Choosing a tower and
then abandoning it (refused, released, timed out) changes nothing.

### Kingdoms and tasks

Geography belongs to one kingdom. Entering a different kingdom discards the old
spotlight and velocity and restarts at that kingdom's main castle with a small random
velocity, so one kingdom's coordinates never influence another. State is kept per
task, in memory only (nothing is persisted), so two tasks in the same kingdom each
follow their own productive ground. The timing generator ([`timing.md`](timing.md)) is
not reset by a kingdom change: timing and geography have different lifecycles.

## `closest` and `random`

* `closest`: the nearest ready tower by Manhattan distance, always. Deterministic.
* `random`: any ready tower, uniformly.

## Tuning

All parameters are named defaults in `SpotlightParams`. Only `home_exponent` is worth
knowing about, because the specified formula (`p = 1`) is a weak bias: towers get more
numerous with distance about as fast as their weight falls, so picks spread across
distances roughly evenly on a log scale and a minority are always far.

Simulated on Pingpoko's 360 sand towers (35 to 61), 15 commanders, 3 hour cooldown and
33 s of march per tile (measured from its ledger), over 8.4 hours, averaged over eight
seeds. This is a model, not a measurement:

| Order | Attacks per hour | Mean distance | Mean march | Farthest pick |
| --- | ---: | ---: | ---: | ---: |
| Old `advanced` (never-attacked first, then oldest) | 28 | 30.4 tiles | 998 s | 46 tiles |
| Spotlight, specified (`p = 1`) | 27 | 32.1 tiles | 1,054 s | 67 tiles |
| Spotlight, `p = 2` | 30 | 28.2 tiles | 926 s | 65 tiles |
| Spotlight, `p = 3` | 32 | 26.2 tiles | 861 s | 65 tiles |
| Strict nearest first (what `closest` does, but by straight-line distance; `closest` itself uses grid distance, which differs slightly) | 39 | 21.3 tiles | 699 s | 33 tiles |

So the specified spotlight gives spatial coherence and unpredictability, but it does
**not** shorten marches compared with the old order, and it still sends an occasional
very long march. If the goal is fewest and shortest marches, `closest` is the
efficient choice and a larger `home_exponent` moves the spotlight towards it. The
default stays at the specified `p = 1`.

Tests: nearer towers are chosen more often but not always; the spotlight pulls picks
towards itself; a used-up neighbourhood relocates; momentum follows the direction of
recent picks and is clamped; a kingdom change resets position, velocity and heading;
a pick that is never accepted leaves the geography untouched; empty and degenerate
inputs are safe.
