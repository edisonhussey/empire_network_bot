# Kingdom grouping

Which **kingdom** to attack in next, so the bot stops hopping between them. The code is
`empire-core/src/grouping.rs` (the rules) and the few lines in
`empire-daemon/src/direct/automation.rs` that use it. Which *tower* to attack inside a
kingdom is [`target-selection.md`](target-selection.md); this sits above it.

## The problem

Tasks were taken in strict rotation, and each task belongs to one kingdom, so with a fire
task and a green task the bot went fire, green, fire, green. Every attack in a kingdom other
than the one the client is showing needs a `gaa` to enter it first (the `AwaitMap` phase), and
the map has to be re-established each time. The rotation is fair, but it is the most
switching possible.

## What it does

Only the **order** tasks are tried in changes. Nothing is refused or skipped, and with
grouping off the order is the old rotation exactly.

1. **Current kingdom first.** The kingdom the client is in (`account_navigation.current_kingdom_id`)
   is tried first, the others after, each group keeping its rotation. The usual result: the
   bot stays until that kingdom has nothing to send.
2. **A short wait beats a switch.** Before leaving, it asks how long until the current kingdom
   could attack again (a commander home *and* a tower off cooldown, the same figures the
   "Waiting" line already uses). If that is within `patience_s` it sleeps instead
   ("staying in kingdom 0 rather than switching"). Longer than that, it leaves.
3. **A cap so no kingdom starves.** After `max_streak` attacks in a row from one kingdom, the
   other kingdoms are tried first for one decision, and the streak restarts wherever the next
   attack lands.

Fortress tasks are ordered the same way but never wait: their dispatch window is a minute.

## Settings (change without a release)

App-state key `automation.kingdom_grouping`, read when a session starts:

```json
{"enabled": true, "max_streak": 12, "patience_s": 120}
```

| Field | Default | Meaning |
| --- | ---: | --- |
| `enabled` | `true` | `false` = the plain rotation, as before |
| `max_streak` | 12 | attacks in a row before another kingdom goes first; `0` = no cap |
| `patience_s` | 120 | longest wait for the current kingdom before leaving it; `0` = never wait |

Out-of-range or missing values keep the default. These numbers are a first guess, not
measured: if the bot waits too long for a quiet kingdom lower `patience_s`; if one kingdom
hogs the commanders raise nothing and lower `max_streak`.

## Limits

* The streak and the current kingdom are in memory; after a restart the first attack starts a
  new run.
* Commanders belong to one task each, so while the bot waits for the current kingdom the other
  kingdom's commanders idle. That is the price of `patience_s`.
* It never moves the client: the current kingdom is whatever the last map request left it as.

Tests: `empire-core/src/grouping.rs` (ordering, cap, patience, settings) and
`empire-daemon/src/direct/grouping_tests.rs` (the real `next_packet`: stays in the current
kingdom, waits for a short cooldown but not a long one, the cap hands over, off means rotation).
