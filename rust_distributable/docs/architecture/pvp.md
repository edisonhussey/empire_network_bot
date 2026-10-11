# PVP tab: other players' movements

The PVP tab lists movements other players make that concern the account or its alliance,
recorded from the `gam` stream. The Development tab draws the ones still travelling.
Packet facts and their limits are in
[`../network_packets/incoming_gam.md`](../network_packets/incoming_gam.md),
[`../network_packets/mrm.md`](../network_packets/mrm.md) and
[`../network_packets/alliance_fields.md`](../network_packets/alliance_fields.md).

## What is recorded

Every movement in a `gam` except the account's own outward ones (the attack ledger already
holds those): alliance to enemy, enemy to alliance, enemy to the account, and anything else the
server shows. One row per movement, keyed by `(account_id, mid)`.

| Column | Source |
| --- | --- |
| `direction` | `incoming`, `outgoing`, `internal` or `other` (below) |
| `owner` | `mine` if the target `(kingdom, x, y)` is one of the account's castles found at initialisation, otherwise `alliance` |
| `sender_*`, `target_*` | player id, name, alliance id and name from `O[]` (matched by `OID`), coordinates and castle name from `SA` / `TA` |
| `army_size` | sum of the unit counts in `GA.L`, `GA.M`, `GA.R`, `GA.RW`; `NULL` when the packet has no units |
| `duration_s` | `TT` |
| `detected_at_ms` | when this app first saw the MID |
| `land_at_ms` | `detected_at_ms + (TT - PT)` seconds |
| `retreated_at_ms` | set by a retreat; final |
| `kind`, `flag` | `M.T`, `M.D`, kept uninterpreted |

### Direction

Judged from the account's own identity (`gbd`: player `gpi.PID`, alliance `gal`) and the
alliance on each end of the movement:

| Sender in our alliance | Target ours or in our alliance | Direction |
| --- | --- | --- |
| no | yes | `incoming` |
| yes | no | `outgoing` |
| yes | yes | `internal` |
| no | no | `other` |

The target counts as ours when its coordinates match an owned castle or its player is the
account. A movement whose sender is the account is not recorded at all. Without a recorded
identity (no `gbd` yet) a `gam` is skipped rather than guessed, because a classification is
kept for good.

## Learned once

The MID is the identity. A later packet for a known MID never changes its timestamps, direction
or owner; it only fills in a value that was still unknown (army size, a name). The same MID can
arrive first in the older `GS` form and later with `GA`, so the army can legitimately appear
late.

## State

State is not stored; it is worked out from the clock when read:

* `retreated`: `retreated_at_ms` is set. Final; a MID is never reused.
* `active`: not retreated and the landing time is in the future.
* `landed`: otherwise. This is the estimated time passing, not a confirmed arrival.

A retreated movement is never "landed": its landing time was only an estimate and it did
not happen, so the tab shows when it was recalled instead and the counts keep it under
`retreated`.

A retreat is read from an **inbound** `mrm` frame with status `0`, and it applies to any
recorded MID whoever owns the movement (the frame names only the `MID`). A request the app
sends is not treated as a confirmed retreat. Whether the server pushes an `mrm` (rather than
something else) when another player retreats has not been observed in a capture; until it is, a
called-back alliance attack may stay `active` until its time passes. An `mrm` naming a MID we
never recorded (our own marches, or one seen before we connected) changes nothing.

## Not kept in memory

Only a page of rows is ever held, in the service and in the window.

* `GET /v1/pvp` returns one page (default 50, at most 200), newest first, with a cursor
  `<detected_at_ms>:<mid>` for the next. Filters: `direction`, `state`, `owner`, `kingdom`,
  `account_id` (default: the connected account, else the most recently seen one).
* The first page also returns whole-history counts per direction and state, computed in SQL, so
  the summary row is true while only a page is loaded.
* The window fetches the newest page when the tab opens, refreshes it every five seconds while
  open, and fetches older pages only on request. Countdowns tick locally from the landing time.
  When the tab is hidden all timers stop.

## Development tab

The map has a kingdom chooser ("Follow the bot" or one kingdom). Every active movement in the
shown kingdom is drawn as a bold arrow, green when the sender is in the alliance and red when it
is anyone else's (attacks on the account included). The colour is the sender's allegiance, so
`outgoing` and `internal` movements are green and `incoming` and `other` are red. Each has a
filled dot at the sender and a ring at the target, labelled `name | alliance`. They exist on the
map only while active, and are placed from the landing time on each animation frame, so nothing
is requested per frame. Only `state=active` rows are fetched, every three seconds.

## Storage

`pvp_movement` and `account_alliance` (migration V21). About 250 bytes a row, kept indefinitely:
the history is the point of the tab. Indexed by `(account_id, detected_at_ms, mid)` for paging
and `(account_id, kingdom_id, land_at_ms)` for the map. Not cleared by "reset account", which
keeps history by design.

## Code

| Where | What |
| --- | --- |
| `empire-core/src/pvp.rs` | Parsing and classification (pure, tested against the captured packets) |
| `empire-core/src/store/pvp.rs` | Persistence, paging, counts |
| `empire-daemon/src/direct/pvp.rs` | Connects the inbound stream to the store |
| `empire-daemon/src/pvp.rs` | `GET /v1/pvp` |
| `desktop/src/views/pvp-model.js`, `pvp.js` | Tab rules (tested) and view |
