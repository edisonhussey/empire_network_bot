# Kingdom transfer

Sending troops from one of the account's castles to another kingdom, by hand, from the Castle Overview, with
an optional "skip the journey" that spends minute-skip items. It is a **manual action that runs only while
the bot is off**. Packets: [`../network/kut.md`](../network/kut.md), [`../network/msk.md`](../network/msk.md),
[`../network/kpi.md`](../network/kpi.md). Prices, timings and the live proof:
[`../game_infrastructure/kingdom_transfer_rules.md`](../game_infrastructure/kingdom_transfer_rules.md).

## Who owns what

| Piece | File | Owns |
| --- | --- | --- |
| Rules | `empire-core/src/transfer.rs` | Packets, parsers, the cost, [`check`](#the-checks), the pause window, the error budget. Pure, no I/O |
| Game data | `empire-game/src/units.rs`, `rules.rs` (rows generated into `generated/units.rs`, `rules.rs`) | Per-unit `kingdomTravellingCost`, each kingdom's tax and journey time, the skip items; built by `utils/scripts/build_tables.py` from the items file ([`../game_data/tables.md`](../game_data/tables.md)) |
| Storage | `empire-core/src/store/transfer.rs`, migration V26 | `kingdom_transfer` jobs; coins, kingdom state and skip stock as snapshots in `app_state`; `transfer_snapshot` |
| Session | `empire-daemon/src/direct/transfer.rs` | The runner: observes every packet, sends one step at a time |
| API | `empire-daemon/src/transfer.rs` | `GET/POST /v1/transfers`, `/check`, `/refresh`, `/{id}/cancel` |
| Window | `desktop/src/views/transfer.js`, `transfer-model.js` | The widget (a projection and a form); nothing computed here |

The window never sends a game packet. It saves intent; the session decides when it is safe to send.

## Lifecycle of a transfer

```text
queued -> [paced pause] -> checks -> kut -> sent -> reply status 0 -> travelling
       -> [paced pause -> msk]* -> landed (done)        (a kpi with no UT for the target)
queued/sent/travelling can also end as failed; only queued can be cancelled
```

* `queued`: stored by `POST /v1/transfers` after the API ran the same checks. Dropped as `failed` if it has not been
  sent within five minutes, so a request made while the bot was on can never fire hours later.
* `sent`: the `kut` is out. `travelling`: the reply (or a `kpi`) shows the transfer in `UT`. `done`: the next `kpi`
  has no `UT` for the target kingdom.
* One transfer is open per account at a time, and the server shows one unit transfer per target kingdom.

## The checks

`transfer::check` runs twice with the same code: in the API (so the window says no at once) and in the session
right before sending (the state may have moved). It returns the cost and every reason it should not go:

| Check | Why |
| --- | --- |
| at least one unit, counts above zero; target is another kingdom | shape of the request |
| units have a known travel cost | an unpriced unit (a tool, say) cannot be sent from here |
| the account owns the source castle in that kingdom | |
| stock read within ten minutes **and after the last transfer from that castle**, and enough of each unit | the server refuses what the castle does not hold |
| coins known and at least the cost | |
| target kingdom unlocked, nothing already travelling to it | |
| bot (attack or recruitment) off; the castle's troops not out as support | the bot never shares the troops or the connection |
| the skip item exists, and the journey needs at most ten of it, and the stock covers it | |

An unknown (stock, coins or kingdom state not read yet) is never a pass. Some problems only need a moment
(unread stock, a bot still stopping, support out): the session waits, asking for a fresh reading. Anything the server
would refuse fails the job without sending a packet, and a failure here does **not** count as an error.

## State funnels, no waiting

`TransferAutomation::observe` takes every inbound packet and stores what it carries, each in its own place:
`gcu` coins, `kpi` kingdoms and transfers (a snapshot: replaced, never merged), `sce` skip stock, wherever they
appear (their own packets, `gbd` at login, the `kut` and `msk` replies). All are absolute numbers from the server,
so order hardly matters and a repeat changes nothing. The job reads that state; it does not wait for a reply
from another funnel. The `kut`, `msk` and `kpi` replies additionally advance the job.

A reply that never comes is **reconciled, not repeated**: after a timeout a `kut` in doubt is followed by a `kpi`
read; if the transfer is there it went out, if not it did not (the job fails), and the `kut` is never sent twice.
After a restart a job in `sent` is reconciled the same way before anything else.

## Pacing and the error budget

Every pause is a draw from the shared wave generator (`Wait::Transfer`, floor 1 s) compressed by
`transfer::paced` into 1 to 12 s: the randomness is kept, the minute-long tail is not (someone is watching).
Sends wait for the connection to be free (the station's gate open, nothing mid-sequence). Two server refusals
(`kut` or `msk` with a non-zero status) inside ten minutes pause new sends until the older one ages out.

## Skips

With a skip item chosen the runner sends one `msk` per pause while the stored `kpi` still shows the transfer
travelling, stops when the item runs out or one more than the journey needs have been sent, and ends the job when a
`kpi` shows no `UT`. Without one it just waits, reading `kpi` every 90 s to notice the landing. A refused skip leaves the
troops travelling.

## The widget

In the Castle Overview, under the castles: choose the source castle (or press "Send troops" on a castle card),
a target kingdom (unlocked ones, with the tax and journey time), counts per troop type (an "All" button), and
the journey (wait, or skip with an item, showing stock and how many it uses). A live line shows the coins,
cost and journey from `/transfers/check`, and any problem in red. "Send troops" is enabled only when the service
sees none. Recent transfers show their phase and a sentence; a queued one can be cancelled, one already out
cannot be recalled.

## Limits

* Troops only; tools have no stored travel cost.
* Only `MS5` has been proven (the rest of the skip items follow the same packet), and error statuses are not
  yet collected.
* The castle-held check in the API cannot see support the session knows about; the session repeats it.
* Not run against the live server through this code: the packets and numbers are those proven by hand through
  `proxy_packet_monitor` on 2026-10-17; the runner is tested on a fake clock with those exact packets.
