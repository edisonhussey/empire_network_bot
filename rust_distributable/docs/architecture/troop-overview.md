# Troop overview, commander ranges and reconnecting

## Troop overview

A live table of every usable commander of the account in view: what it is doing, where, with
how many troops, how much loot it carries and when it is free. It reads what the bot already
keeps (`commander_state` and the march ledger, `empire-core/src/store/troops.rs`); nothing is
stored for it and nothing is sent to the game. `GET /v1/troops?account_id=`.

| State | Meaning | Times |
| --- | --- | --- |
| Outbound | troops are travelling to the target | *Lands* from the acknowledged travel time, *Free* from the commander's hold |
| Returning | fought; coin and rubies shown are what they carry home | *Free*: the return time plus the rest the bot gives |
| Sending | a `cra` left and has not been answered | |
| Support | lent to an auto-station march (a `station:` lease) | |
| Inactive | at home | |

Commanders are listed by their human number (1, 2, 3...: the n-th commander of the account's
own roster, in the order the game shows them), not by the internal id. A commander in a range
kept for support is marked *Reserved*.

A return whose packet was never seen (the session was interrupted for minutes) is bounded
instead of left hanging: a return takes no longer than the trip out, so a march is home by
`sent + 2 x duration + 15 s`. Past that the march is lapsed (`march.lapsed` in the ledger), the
commander is free again and the map drops it; before that it reads *Returning (est.)* with
"up to" a time. The same bound serves the Development map. A late real return still matches.

A hold that has run out reads as inactive even if the ledger still says outbound: the ledger
can lag the game, and a commander the bot would send again is not "out". The window polls every
three seconds and counts down from the timestamps every second. Recent marches are listed under
the table. The existing Overview tab is now called **Castle Overview**.

## Commander ranges in an attack bot

A task owns a range of commander numbers. The builder shows one row per task with *from* and
*to*, for example task 1 `1-10`, task 2 `11-20`, task 3 `21-30`:

* A task follows the one above it until its numbers are edited ("Follow above" goes back).
* Numbers may be skipped (task 3 at `25-30` leaves `21-24` unused); they are lent to no one.
* Two tasks can never share a commander: strict sections still hold, and the builder, the
  service and import all refuse an overlap, naming the two ranges.
* A bar under the rows shows each task and the unused numbers; a bot needs as many commanders as
  its highest number.

The rule is one function, `resolve_commander_ranges` (`empire-core/src/planning.rs`), mirrored by
`desktop/src/views/commander-ranges.js`. A bot saved before this has no starts and means exactly
what it did (counts following each other). Exports carry `commander_start` only when it was set.

## Support in an attack bot

An attack bot may carry one support object (never two: it already says where troops wait for
every castle). Its commanders are reserved on the same board as the tasks, as horizontal blocks
along the commander numbers, for example `support 1-9 | task 10 | support 11-20`, so a support
march always finds a free one and no attack takes them. The board is horizontal because the
spread reads at a glance; click a block to change its first and last number or move it left or
right past its neighbour. While that attack bot runs its support replaces the account's separate
auto-station choice, and the support cannot be deleted while a bot uses it. Reserved commander
ranges follow the same no-overlap rule as tasks.

## Starting with no attack bot

The Control Panel's attack bot choice may be "None": a recruit bot or the auto-station can run
alone. Starting needs at least one of the three. Choosing none forgets the earlier attack bot, so
the window shows "none" next time.

## Presets, opened

Manage presets can open any preset to show what it is made of (an attack wave by wave and flank
by flank, a task's source, target, algorithm and travel, a bot's commander ranges, a support bot's
destinations) instead of only copying JSON. The text is built from the same draft the preset is
exported as (`views/preset-details.js`).

## When a session reconnects

The supervisor retries a **lost link** (a socket error, or 90 s of silence) twice, about a minute
and five minutes later. It does not retry:

* **The operator ending the session** (Log out): the session's task, supervisor included, is
  stopped and `connection.logout` is logged. A test shows a task that would loop forever stops
  for good when ended.
* **The server closing the session**: a close frame is logged as `connection.server_closed` with
  its code and reason and the session ends. A retry after a server-side close would sign the
  player out again when the cause is the account being used elsewhere, and a login is exactly the
  event the game notifies the player about.

Whether the game sends a close frame for a second login has not been captured; if it only resets
the connection it still reads as a lost link. The event log says which happened.
