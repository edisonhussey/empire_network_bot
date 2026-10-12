# Auto-station

When an owned castle is attacked by another alliance, its troops are sent out as
support about five seconds before the landing and called back once the danger has
passed. A *support bot* is only the spot to send them, one coordinate per kingdom; a
kingdom with none is never stationed. The castles themselves are known from sign-in,
so nothing about them is configured.

Packet facts: [`../network_packets/station_sequence.md`](../network_packets/station_sequence.md).
Rules (pure): `empire-core/src/station.rs`. Persistence: `store/station.rs` (migration
V22). The session part: `empire-daemon/src/direct/station.rs`. HTTP: `daemon/src/station.rs`.

## Reserved commanders

An attack bot's support (see [`troop-overview.md`](troop-overview.md)) names commanders kept
for it. `choose_support_lid` prefers a reserved commander that is free, then a reserved one
back before the support has to leave, and only then the usual order; a reserved commander is
never given to an attack, so one is always there.

## What triggers it

A `gam` on the direct connection whose target `(kingdom, x, y)` is one of the account's
own castles, whose target player is the account, and whose sender and receiver have
**different** alliance ids. Both player records and both ids must be present, and `-1`
(no alliance) against `-1` is skipped. This is an operational rule, not a verified
attack flag: the packets carry none. The account's own outward marches never match. The
landing is `received_at + (TT - PT)`, fixed at first sight; a repeated `gam` never moves
it or starts a second job.

One support march covers every attack on the same castle while it is out: a second
attack joins the existing job, and the recall waits for the *last* one. An attack that
lands earlier than the one a job was planned around moves a job that has not yet sent
anything. Nothing is recorded as a threat while no station bot is running.

## The timeline

All deadlines are measured from a *safe* landing: the estimate less one second (`PT` and
`TT` are whole seconds) less the measured one-way delay (smoothed from the round trips
of these requests, 250 ms until measured). With `S` the safe landing and `D = S - 5 s`:

| When | What |
| --- | --- |
| `D - ~2 s` (normalised) | `dcl`: every castle's stock; `PID` must be the account |
| `D` | `sdi`: opens the support page for the attacked castle |
| `sdi` reply + a short pause | `cds`: the support march, with the fresh stock |
| after the landing + 0.9 to 4.5 s | `mcm`: recall, by the support MID |
| or promptly after an inbound `mrm` | the same recall, once every covered attack is retreated |

A map-tile request precedes `dcl` only when the client is not already on that kingdom's
map (a client in a castle or recruiting view would not open a support page from there).
It is dropped, then the `dcl`, when there is no time for them.

### Why waits are normalised

The human-paced wait generator is heavy-tailed on purpose: most draws are a few seconds
and some are minutes. Spacing steps inside a five second window with raw draws would miss
the landing on one long draw. `normalise_gaps` keeps each draw's randomness but maps it
monotonically into a window that fits (`dcl` to `sdi` 0.9 to 3 s, the pause before `cds`
0.35 to 1.8 s, the recall 0.9 to 4.5 s), and when the gaps together exceed the time left
it scales only the part above the floors. If the floors alone do not fit, optional steps
go in order of how little they matter (the map tile, the `dcl` cross-check) and `sdi` goes
at once. The pause before `cds` is also held to half of what is left before the last safe
send. A protocol floor is never compressed. The draws come from the station's own
generator and appear in the Development tab's histogram as "station step".

### Measured, not guaranteed

The feature is judged by how long before the estimated landing the `cds` is acknowledged
(`lead`, stored per job). Over 72 simulated runs (notice of 9, 12, 30 and 181 s; round
trips of 100, 400 and 800 ms; random draws) the acknowledgement came **3.0 s at the
earliest** and typically 4.4 s before the landing. That is a simulation of the timing
logic with a fake clock and fixed round trips, not live latency; no timing algorithm can
guarantee a five second completion over an unbounded link. Live leads are recorded in
`station_job.lead_ms` and shown in the Development tab.

## Sharing the connection

The station comes first and the rest of the bot gives way, without being abandoned:

* **Gate.** `open`; `quiet` from 45 s before a job's first step (sequences already
  running may finish, none starts, no new map-scan or fortress window); `owned` from
  1.5 s before the first step until the `cds` is acknowledged (nothing else sends). An
  attack handshake held at the gate resumes afterwards, or is released safely if the
  context moved. A held recruitment resumes or restarts through its own context check.
  The Overview's `dcl` is sent only when the gate is open and nothing is mid-sequence.
* **Single flight.** One station request is in flight at a time, the earliest landing
  first. Two castles attacked together are served one after the other inside their own
  windows.
* **Same troops.** A `cds` waits for an unanswered `cra` from the same castle, since
  both draw on those troops, unless the window is nearly gone. Once support has left a
  castle, **no attack may be sent from it** until the troops are home again (the
  recall's `TT - PT`, plus a margin): a `cra` for troops that are away is refused, and
  repeated refusals have locked the account. A `cra` already waiting is released the
  way a stale map context is.
* **Commander.** One real LID is leased when the threat is seen: a free one no running
  task uses, else the last free one of the task with the most, else one back well before
  `D`; none means the job is blocked and recorded, never sent with a guess. It is
  revalidated just before `cds`, held until the recall's return time, and freed at once
  if nothing leaves.

## Uncertainty

* A silent `dcl` is dropped and `sdi` stands alone. A silent `sdi` is asked once more if
  time allows, then the job is closed as missed.
* A silent `cds` is **never sent again**: the job becomes *uncertain*, keeps its
  commander and its castle hold, and is reconciled if the account's own movement to the
  destination appears in a `gam` (its MID is then adopted and recalled). After 30
  minutes unconfirmed it is closed as `uncertain`.
* A retreat that arrives while the `cds` is unanswered is kept; the recall is sent once
  the support MID is known, never guessed.
* A silent `mcm` is recorded as `saved:recall_uncertain` and **not repeated**; check the
  march in game. A refused one is recorded with its status.
* Refusals are recorded with their status and not retried unchanged.

## Persistence

`station_job` keeps one row per stationed castle with the numbers the UI needs and a
state blob, so a job survives a restart: a job with support out carries on to its recall
(promptly, if overdue); one that had sent nothing and whose landing passed is closed as
`missed:session_gap`; one whose `cds` was in flight becomes uncertain. Attack MIDs already
handled are reloaded so a `gam` repeated at login does not make a second job. Recalls
keep running when the station is stopped in the Control Panel; stopping cancels jobs that
have not yet sent anything and frees their commanders.

## Overview

Every castle's troops, tools, food and mead as `dcl` last reported them
(`castle_inventory`, one row per castle). The tab reads the stored reading; it asks for a
fresh one (a flag the session loop notices) when it opens with a reading older than two
minutes or none, and when the button is pressed, never more often than every 15 seconds.

## In the window

* **Station > Create support bot**: name and, per permanent kingdom, a destination.
* **Control Panel, step 2**: one support bot or none, chosen with the attack and recruit
  bots and started and stopped with them.
* **Development**: a frequency chart of the human-paced waits (log-spaced bins, the
  share inside 1 to 5 s printed), a Station panel (gate, jobs, the time before the
  landing at each step), and on the kingdom map the attacked castle ringed, a moving
  dashed line to its destination, the support marker and countdowns to the recall and
  the landing.
