# Open defects

The live register. Each entry keeps the evidence that found it, so it can be
re-checked before and after a fix rather than taken on trust.

Status values: **open**, **fixing**, **fixed (build)**, **wontfix**.

---

## D1 — A refused target is retried forever · open

**Symptom.** The bot sits at `Idle — waiting for a free commander` with 0 attacks
recorded, while the log shows the same request repeating every ~16 seconds.

**Evidence.** `bot_detail` read `adi rejected with status 6; retrying after
backoff`, and the recorded pairs were identical:

```
07:47:08  C→S  adi  {"KID":1,"SX":593,"SY":613,"TX":321,"TY":399}
07:47:24  C→S  adi  {"KID":1,"SX":593,"SY":613,"TX":321,"TY":399}
07:47:40  C→S  adi  {"KID":1,"SX":593,"SY":613,"TX":321,"TY":399}
```

A later read through the lab panel showed the same shape against a *different*
target — `{"KID":1,"SX":593,"SY":613,"TX":419,"TY":770}` at 07:57:45 — so this is
the general case, not one bad coordinate.

**Tell for finding these pairs.** A *rejected* `adi` comes back as a
`server_to_client` row with an **empty payload**; a successful one carries data.
That makes a rejection visible in the packet log without reading the status code.

**Cause.** On rejection the daemon calls `release_fortress_target`, which clears
the reservation. Nothing records that the target was refused, so the next pass
selects the same one. It burns the whole one-minute window on one target and
then goes idle.

**Fix.** Record a short-lived skip for a `(kingdom, x, y)` the server has
refused, and exclude it from selection until the skip expires.

**Open question.** What status 6 *means*. If it is "this one is not attackable"
then retiring it is right. If it is an envelope problem the fix belongs
elsewhere. Needs a capture with a 6 in it.

---

## D2 — An expired fortress window is never re-armed · open

**Symptom.** The dashboard says `Next fortress: Ready now` and nothing happens.

**Evidence.** Buckets over `fortress_target`:

```
expired            31     available_at_ms is in the past
future           1454
inside the minute   0     the only bucket the dispatcher acts on
```

**Cause.** Dispatch requires `available_at_ms >= now − 60_000`. When the server
reports `cooldown_remaining_s = 0` the fortress *is* attackable, but
`upsert_fortress_targets` deliberately preserves a past `available_at_ms`
("an expired window is never re-armed"), so it can never be dispatched again.

**Fix.** When the server reports cooldown 0, set `available_at_ms = now` so the
fortress gets a fresh minute. This must ship *with* D1, or a target the server
refuses would be retried every minute instead of once per window.

---

## D3 — The sweep blocks all automation · open

**Symptom.** Windows open and are missed, which is how D2's 31 expired rows
appeared.

**Cause.** `automation_tick` is gated on `fortress_initialization_complete`. A
whole kingdom is 578 requests at ~1.15s ≈ 11 minutes during which nothing else
runs.

**Partly addressed (0.1.31).** The gate no longer costs anything once the walk is
finished. Fortress coordinates never change, so `Store::outstanding_fortress_sweeps`
reports only the sweeps that still have blocks, and an account already walked
skips the phase outright — no settle delay, no kingdom-switch pause, and the
session goes straight to ready instead of flashing `discovering_fortresses`.
Measured on the live account: four of six sweeps were already at `289/289`, so
those minutes were pure waiting.

That fixes the *finished* case only. The first walk of a fresh kingdom still
blocks automation for its full duration, so the original fix below is still
needed.

**Remaining fix.** Let automation start once the near ground is swept and
continue the sweep in the background. Needs a decision about how much ground
counts as "enough" before attacking begins. The span-60 change in
`fortress-discovery.md` §6 would halve the exposure (289 instead of 578) without
answering the question.

---

## D4 — Two labels misreport state · open

**"Doing: Idle — waiting for a free commander."** `waiting` is the daemon's
generic "nothing is dispatchable" state, mapped to that string in `main.js`. It
never checks commanders. `commander_state` had **0 rows** at the time, so every
commander was free. The useful text — `bot_detail` — is only a tooltip.

**"Next fortress: Ready now."** Computed as `available_at_ms <= now`, which is
true for all 31 *expired* fortresses. The dashboard advertises a target the
dispatcher is designed never to touch.

**Fix.** Show the reason from `bot_detail`, and distinguish "free now" from
"freed, and we can no longer claim it".

---

## D5 — The packet log is a rolling buffer · open

**Symptom.** Evidence disappears. A recording that showed eleven 42-wide windows
now shows only 13-wide client tiles, because older rows aged out.

**Consequence for method.** Anything that needs to be re-checked later has to be
copied out to `development/logs/` at the time, not left in the table.

---

## D6 — `rbc_target` is empty for Sands · unexplained

**Evidence.** `SELECT COUNT(*) FROM rbc_target WHERE kingdom_id = 1` → 0, while a
`Sands` attack bot was configured and `accounts` reported 362 targets overall.

Not yet diagnosed. Could be the radius filter rejecting everything, or the map
scan not reaching any RBC. Worth checking before anything else in the ordinary
farming path is trusted.

---

## D7 — An injected `gaa` can be swallowed by the runner · open

**Symptom.** A hand-sent window appears to be answered but the runner's map walk
advances as though it had received its own tile's reply.

**Cause.** The runner matches an `active_base_scan` reply on `KID` alone
(`packet.payload.get("KID") == Some(active.kingdom_id)`), and a fortress probe
matches on `KID` plus `status == "0"`. Nothing checks that the reply corresponds
to the window that was asked for, and a `gaa` reply does not echo its window.
An injected window for the same kingdom therefore satisfies either matcher.

**Impact.** Cosmetic while probing by hand — one map window gets marked learned
without having been requested. It would matter if injections were ever used to
drive the bot rather than to observe it.

**Observed.** 2026-10-05: an injected 42-wide window for KID 1 landed between two
of the runner's own 13-wide tiles (sequences 4550, **4552**, 4554).

---

## Not defects, but they look like it

- **`fortress_scan_frontier` exists and is empty.** Retired per-coordinate queue,
  kept because the schema list is re-applied every launch and V13/V14 reference
  it. Nothing reads it.
- **`transport_connected: false` after an install.** A service swap drops the
  session by design.
- **`adb`/`adi` retries with no commander load.** See D1 — the retry is the bug,
  not the commander state.
