# Response health

Telling a quiet, healthy exchange from one that has gone wrong. Code:
`empire-daemon/src/direct/health.rs`, used by the attack state machine.

## What counts

| Outcome | Counts towards the tolerance |
| --- | --- |
| Expected empty response (a valid empty tile, say) | no |
| Explicit server rejection (a status such as 95) | no |
| Connection loss (the link was silent) | no; the reconnect supervisor handles it |
| Missing response / timeout on a live link | **yes** |
| Unexpected null (status 0 with no payload) | **yes** |

An empty payload is not treated as a failure by itself.

## The policy

Qualifying incidents are kept for one **rolling hour**. The default tolerance is **two**:

1. First: recorded, warning shown.
2. Second: recorded, "tolerance reached" shown.
3. Third within the hour: **no new actions start.** The bot goes to a paused state
   (`paused_on_errors`, detail `PAUSED: ...`).

A pause never exits the process or discards data, and work already on the wire resolves.
It is also **not undone automatically**: stopping the mode is the operator action that
clears it. Time passing does not.

Configure the tolerance by setting the app-state value `automation.null_tolerance`
(an integer; default 2, never below 1). It is read once when a session starts.

## Relationship to the older safety pause

The earlier rule (two operational errors in five minutes pauses attacks until the
window clears; five disables the mode) is unchanged. This is a longer-horizon check on
the same timeouts. Refusals such as 95 remain exempt from both.

## Visibility

Each warning and pause is an `event_log` row (`health.null_response`, `health.paused`)
with the count and tolerance, and the Development tab lists the incidents of the last
hour with the qualifying ones marked. See [`database.md`](database.md).

## Tests

Which outcomes qualify; the third incident pausing; incidents expiring after exactly one
rolling hour; slow trickles never pausing; non-qualifying noise listed but not counted;
a pause surviving the passage of time until the mode is stopped; the configurable
threshold; the bounded incident list; timeouts on a silent link and ordinary refusals
not counting.
