# Attack scheduling: towers, commanders and cooldowns

Findings from a long Sands RBC run where only 2 of 35 commanders were in use.
Code: `crates/empire-daemon/src/direct/automation.rs`, with target state in
`crates/empire-core/src/store.rs`.

## Two things limit throughput

An attack needs a **free commander** and a **ready tower**. In the failing run
commanders were never the limit: for 2.5 hours sand commanders 11-24 sat home
while the bot launched 2 attacks per ~5 minutes. Towers were the limit, and the
bot could not see that.

## Towers: the server tells you their cooldown

* Every RBC row in a `gaa` carries the remaining cooldown ([`gaa.md`](gaa.md)).
* Stored as `rbc_target.server_free_at_ms` (0 = ready). Any map response
  refreshes it for known towers.
* A tower is selectable only when all of these hold:
  `reserved_until_ms <= now`, `server_free_at_ms <= now`, and our own ledger
  cooldown (landing + 3h, capped at sent + 4h, + 5 min) has passed.
* `next_rbc_ready_ms` is the minimum of the same three, per task range. The
  scheduler uses it to sleep exactly until work exists.
* Before this, the bot learned cooldowns only by being refused (`95`) and then
  parked the tower for an hour regardless of the true remaining time.

### On a `95`

1. Hold the tower 60 s.
2. Send a `gaa` tile read centred on it.
3. The response updates `server_free_at_ms`. Cooling: move on. No cooldown shown
   despite the refusal: park 30 min.
4. A missing tile reply parks the tower and is **not** an operational error.

## Commanders

* Identity is the server `LID`. Usable LIDs are a curated list
  (`hunt::USABLE_COMMANDER_LIDS`); the roster in `adi` is not availability.
* A commander is **reserved when the CRA is sent** (5 min provisional hold), not
  when it is acknowledged. The ack replaces it with `now + travel + return`
  estimate, and `cat` (matched by target + LID, never by `MID`) sets the real
  return time. A rejected CRA releases it after 15 s.
* A task's commanders are an allocation, not a wall. If a task has a ready
  tower and all its own commanders are out, commanders of a task with no ready
  tower are lent to it. A task with work keeps its own.

## Waiting

When nothing can be sent the scheduler computes, per task, the later of "next
tower ready" and "next commander home", sleeps until the earliest (1.5-30 s,
plus jitter), and writes the reason into the status line, e.g.
`Waiting - Sand 61: 14/17 commanders home, next tower in 6m 12s`.
Reading it: *next tower in ...* is a tower shortage; *tower ready, next
commander in ...* is a commander shortage.

## Leases

Selecting a tower leases it for 12 minutes. A handshake abandoned before the
attack (no commander free, map context lost, mode stopped) releases the lease
immediately.

## What caps utilisation

Roughly `towers / cooldown`. 316 level-61 towers on a ~3h cooldown is ~100
attacks/hour at the very best, less when other players hit them. More
commanders cannot raise that; more towers can (more kingdoms or levels as
tasks).

## Diagnosing a quiet run

* Status line: tower shortage vs commander shortage (above).
* `attack_ledger` per hour: `sent` counts and distinct `lord_id`.
* `commander_state`: commanders with `available_after_ms` in the past are home.
* Only the last 200 `network_message` rows are kept, so refused ADIs (`95`) are
  not recoverable after the fact.

## Not changed (known limits)

* One handshake is in flight at a time (ADI, 2-4 s, CRA, ack). That is roughly
  3 attacks/min, enough for ~20 commanders on a ~9 min round trip.
* Two operational errors in 5 minutes pause attacks until the window clears;
  five disable the mode. Target-specific refusals (`95`) are excluded.
