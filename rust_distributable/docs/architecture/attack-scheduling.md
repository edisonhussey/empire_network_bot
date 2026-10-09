# Attack scheduling

How the bot decides what to attack and with whom: towers, commanders, waiting and the safety pause. Connection loss is covered in [`connection-supervision.md`](connection-supervision.md); the packets themselves are in [`../network/`](../network/).

Attacks are a pipeline with two independent inputs: a **ready tower** and a
**free commander**. The scheduler (`direct/automation.rs`, state in
`empire-core/src/store.rs`) exists to keep both visible, so that when throughput
is low the cause can be read off rather than guessed.

## One handshake at a time

`Idle -> (map open) -> ADI -> wait 2-4 s -> CRA -> ack -> Idle`. Only one
handshake is in flight; the global CRA floor (4 s) and the post-ack pause live in
`empire-core/src/pacing.rs`. That is roughly 3 attacks a minute, enough to keep
~20 commanders busy on a ~9 minute round trip. A refusal or timeout returns to
`Idle`; nothing else mutates the pipeline.

## Towers: trust the server's cooldown, not our guess

* Every RBC row in a map response carries its remaining cooldown
  ([`gaa.md`](../network/gaa.md)). It is stored as
  `rbc_target.server_free_at_ms` and refreshed by **any** map response, scan or
  not, for towers already known. It never adds a tower the operator did not scan.
* A tower is selectable only when `reserved_until_ms <= now` (lease / local
  cooldown), `server_free_at_ms <= now`, and the ledger cooldown (landing + 3 h,
  capped at sent + 4 h, + 5 min) has passed. `next_rbc_ready_ms` is the minimum
  of the same three, so the two queries cannot disagree.
* Selecting a tower leases it for 12 minutes. A handshake abandoned before the
  attack releases the lease at once (`release_rbc_target`).
* A refusal with status 95 means our map of that tower is stale. The tower is
  held for a minute, its tile is re-read, and the server's real cooldown replaces
  the guess. A flat one-hour park (the old behaviour) wasted every minute the
  tower was actually ready. A refusal the tile cannot explain parks it 30 min.

## Commanders: reserved early, never lent

* Identity is the server `LID`. The visible commander number is the 1-based
  position in `hunt::USABLE_COMMANDER_LIDS` (a copy of the Python
  `DEFAULT_COMMANDER_LIDS_BY_HUMAN_NUMBER`), and the LIDs skip 1, 4, 5, 12-15 and
  19, so number and LID never line up (human 17 is LID 24, human 29 is LID 36).
  Tasks take consecutive blocks of that table in mode order, so the first 17 are
  the first section and 18-35 the second.
* **Allocation is strict.** A task may send only its own block. An earlier version
  lent idle commanders between tasks; on 7 Oct it sent two crossbow marches under
  human 29 and 30 (LIDs 36, 37), commanders reserved for the other section.
  The point of a section is that the first 17 real commanders carry only the
  first attack type, the next 18 only the second. That is a contract about which
  commander carries which army, not a throughput hint, so a task whose commanders
  are all out waits (even if another section is idle) and the status line says so.
  Throughput is therefore bounded per section by its commanders' round trip, and
  a pool that looks underused is the intended result, not a scheduling fault.
* The `gli.C` list in an ADI reply is the whole roster, not availability, and is
  not used to decide who is free.
* A commander is reserved when the CRA **leaves** (5 minute provisional hold),
  not when it is acknowledged. The ack replaces that with travel time; `cat`
  (matched by target and `LID`, never by `MID`) sets the real return. A rejected
  CRA releases the commander after 15 s.

## Waiting is computed, not polled

With nothing to send, the scheduler takes, per task, the later of "next tower
ready" and "next commander home", sleeps until the earliest (1.5-30 s plus
jitter), and writes the reason to the status line, for example
`Waiting - Sand 61: 14/17 commanders home, next tower in 6m 12s`. *Next tower in
...* is a tower shortage; *tower ready, next commander in ...* is a commander
shortage. Utilisation is capped by roughly `towers / cooldown`: more commanders
cannot raise it, more towers (kingdoms, levels, tasks) can.

## Never send an army that is not there

The `adi`/`abi` reply carries the castle's home inventory (`gui.I`, `[unit, count]`
rows; `gui.TU` is troops currently out). Before a CRA is queued the attack payload's
total per unit is compared with it (`hunt::army_shortfall`). Short: nothing is sent,
the tower and commander are released and the scheduler waits 30 to 60 s for troops
to return (`Waiting for troops to return: unit 607 need 50 have 12`). Troops, not
commanders, can be the real limit: a sand march loses about a quarter of its troops,
so a pool of ~385 crossbows supports only about seven 50-troop armies at once.

Every attack-details reply also writes the stock to the database and ends the status
line with the live figure and burn rate (`unit 607: 21,459 home, 853 out, draining
1,380/h, about 15.5h left`, `LOW, recruit now` under three hours); see
[`database.md`](database.md).

An unexplained refusal is different from a short army. Only 95 (tower cooling), 93
and 256 (commander busy) are routine for a `cra`; any other status (101, 313, 5, ...)
**stops the mode** and leaves `STOPPED: ...` on the status line. On 8 Oct three 101s
in 40 s ended in a 24 h account lock; see `docs/problems/1.md`.

## Errors and the safety pause

Two operational errors in five minutes pause attacks until the window clears;
five disable the mode. Target-specific refusals (95) and tile-read timeouts are
not operational errors. A request that times out while nothing at all has
arrived for 25 s is blamed on the link, not the target: the tower is released,
no error is counted, and an attack that may have left keeps its commander
reserved.
