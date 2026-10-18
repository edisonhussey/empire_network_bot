# Database

One SQLite file (`empire.sqlite3`, found through `empire_core::paths`) holds all
durable state. This page covers how it is organised, what is kept for how long,
and the schema changes made for run telemetry (migration V19). The schema itself
lives in `crates/empire-core/src/store/schema.rs`; behaviour lives in `store/`.

## Why V19 exists

On 8 Oct a run died because the castle ran out of crossbows, and the figure that
showed it (the troop stock, in the `gui` block of every attack-details reply) was
only in raw packets. The packet log is capped at 200 rows (about half an hour), so
the stock history, the per-attack losses and the reconnect history were gone by the
time anyone looked. See [`../problems/1.md`](../problems/1.md). V19 keeps those
numbers in small tables that cannot grow without limit.

## Accounts

Every table that holds history is keyed by `account_id`, so a second account never
mixes with the first. Ids go through `canonical_account_id` (trimmed, lowercase)
on every write and read.

| Scope | Tables |
| --- | --- |
| Per account, already | `attack_ledger`, `commander_state`, `account_commander`, `owned_castle`, `rbc_target`, `fortress_target`, `account_mode`, `account_recruit_bot`, `recruit_castle_state`, `account_navigation`, `map_scan_window`, `fortress_scan_state` |
| Per account, added in V19 | `castle_stock_sample`, `castle_stock_hourly`, `event_log`, `event_hourly`, `network_message.account_id` (nullable; older rows and relay traffic have none) |
| Per account, by key | `app_state` rows named `login.locked_until_ms.<account>` |
| Global | `app_state` `hunt.heartbeat_ms` (one live session per service), the `licence` table (one row per licence, each bound to one account through `licence_activation`), task / profile / mode definitions (shared on purpose, a mode can run on any account) |

To start clean for a test account nothing needs deleting: a new `account_id`
starts with empty history. To remove one account's history:

```sql
DELETE FROM castle_stock_sample WHERE account_id = 'name';
DELETE FROM castle_stock_hourly WHERE account_id = 'name';
DELETE FROM event_log           WHERE account_id = 'name';
DELETE FROM event_hourly        WHERE account_id = 'name';
DELETE FROM attack_ledger       WHERE account_id = 'name';
```

## What V19 adds

### `castle_stock_sample` (raw, 72 hours)

`(account_id, castle_id, unit_id, observed_at_ms, home, out_count)`

Written from every attack-details reply (`adi` / `abi`): `gui.I` is the castle's
home inventory, `gui.TU` the troops currently out, `SCID` the castle. Only the
units the running attack actually uses are stored (for example 607), not the ~20
rows of the full inventory. At most one row per account, castle and unit per
minute; a busy run does not write a row per attack. Rows older than 72 hours are
deleted on the next write.

### `castle_stock_hourly` (summary, forever)

`(account_id, castle_id, unit_id, hour_ms, samples, min_home, max_home, last_home, last_out, last_at_ms)`

Updated on every reply, including the ones the raw table skips. One row per unit
per hour, about 8,800 a year for one unit. This is the long-term curve.

### `event_log` (important events, 90 days)

`(id, account_id, at_ms, kind, detail)`

Rare events worth reading one by one:

| Kind | Meaning |
| --- | --- |
| `connection.connected` | Socket opened (detail: endpoint) |
| `connection.login_ok` | Authenticated |
| `connection.login_rejected` | The game refused the login; detail has the status and the lock expiry |
| `connection.lost` | Socket reset, I/O error, or 90 s of silence; detail is the full error |
| `connection.reconnect_scheduled` / `connection.retry` | Retry 1 or 2 queued / starting |
| `connection.gave_up` | Both retries spent |
| `connection.failed` | A final, non-retryable error (licence, refused login) |
| `connection.closed` | The server closed the connection cleanly |
| `attack.stopped` | The mode was stopped because of an unexplained refusal; detail has the status |
| `health.null_response` | An unexpected missing or null response was counted; detail has the count and tolerance |
| `health.paused` | The third qualifying incident in a rolling hour stopped new actions |
| `licence.needed` | The account authenticated but no installed licence matches it; detail has the server, player id and main castle a licence must be issued for |

### `event_hourly` (counters, forever)

`(account_id, hour_ms, kind, count, first_at_ms, last_at_ms, last_detail)`

Routine events that can repeat many times: `army_short` (waiting for troops),
`cra_refused_<status>` / `adi_refused_<status>` (routine refusals),
`timeout_adi`, `timeout_cra`, `request_timeout_link_silent`. Five hundred
occurrences in an hour are one row with `count = 500`, so a retry loop or a long
wait cannot grow the database.

### `attack_ledger` additions

| Column | Meaning |
| --- | --- |
| `army_json` | Troops the march carried, `{"607": 50}` (troops only, not tools), set when the attack is acknowledged |
| `troop_count` | Sum of `army_json` (the column already existed and was always empty) |
| `troops_returned` | How many of those came home, from the `cat` reply's `A.A` |
| `troops_lost` | `troop_count - troops_returned`, per unit, never negative |

Marches that land while the bot is offline never produce a `cat`, so their rows
keep `troops_returned` / `troops_lost` empty.

### `network_message.account_id`

Which account a captured packet belongs to. The log is still capped at 200 rows
(`RECENT_MESSAGE_LIMIT`); it is a debugging aid, not history.

## V21: the PVP record

* `pvp_movement`: one row per other-player movement, `(account_id, mid)` key, kept indefinitely
  (about 250 bytes a row). Learned once; only an unknown army size or name is filled in later.
  `retreated_at_ms` is final. Indexed for paging and for the map's active query.
* `account_alliance`: one row per account (player id and name, alliance id and name) from the
  login `gbd`.

Neither is cleared by "reset account". See [`pvp.md`](pvp.md).

## V22: auto-station

* `station_bot` / `station_bot_kingdom`: a named support bot and its destination per
  kingdom; `account_station_bot` picks at most one per account and whether it runs.
* `castle_inventory`: the latest `dcl` reading per castle (units as JSON, food, mead, wood,
  stone); one row per castle, replaced each time.
* `station_job`: one row per stationed castle (state blob, support MID, commander, the
  acknowledgement and recall times, and `lead_ms`, the time the support was acknowledged
  before the estimated landing). Kept for good, about 600 bytes a job; finished jobs older
  than 180 days are pruned when a session starts. Not cleared by "reset account".

See [`station.md`](station.md).

## V23: commander ranges

`automation_mode_task.commander_start` (nullable): the commander number a task starts at in an
attack bot. `NULL`, which every bot saved before has, means the task follows the one before, as
the plain counts always did. See [`troop-overview.md`](troop-overview.md).

## V24: support in an attack bot

* `automation_mode.support_bot_id` (nullable): the one support object the bot carries.
* `automation_mode_reserve(mode_id, position, commander_count, commander_start)`: ranges of
  commanders kept for that support. `position` is the slot's place on the board among the
  tasks (`automation_mode_task.position`), so `support 1-9 | task 10 | support 11-20` reads
  back in order. A reserved commander belongs to no task, so no attack is ever given it.

See [`troop-overview.md`](troop-overview.md) and [`station.md`](station.md).

## V25: robber baron victory counts

`rbc_target.victory_count` and `rbc_target.hits_to_next` (both nullable). The server sends a
tower's victory count, not its level; the level is derived from the count and the kingdom
([`robber_baron_level_formula.md`](../game_infrastructure/robber_baron_level_formula.md),
`empire-core/src/dungeon.rs`). `level` stays, because selection filters on it, but it is now
written for **every** kingdom (before, only Sands got one, so a task band in any other kingdom
matched nothing). `hits_to_next` is how many more wins the next level needs.

* A map row sets all three together (`upsert_rbc_targets`), so the server's own number always
  wins on the next scan.
* A **returned attack that brought loot** adds one win (`record_rbc_victory`, called from
  `apply_attack_return` after the return has matched its in-flight march, so a replayed return
  cannot count twice) and recomputes `level` and `hits_to_next`: no map read needed. Other
  players' attacks are only learned from the next scan.
* A row with no count (every row stored before V25 until it is next seen) is never advanced; a
  guessed count would be worse than an old level.
* The migration also drops the stored `map_scan_window` rows (not Berimond's), so every kingdom
  is read once more and the rows fill in. This is a one-time extra scan, as V17 was for fortresses.

## V26: kingdom transfers

`kingdom_transfer`: one row per transfer asked for, from the request to the landing. `phase` is `queued`, `sent`
(the `kut` is out, no answer yet), `travelling`, then `done`, `failed` or `cancelled`; `detail` is the sentence the
window shows; `cost_c1` the coins it will cost; `skips_sent` how many skip items were spent. Kept for good (about
300 bytes a row). The coins, the kingdom state and the skip stock the checks read are small snapshots in `app_state`
(`transfer.currency.<account>`, `transfer.kingdoms.<account>`, `transfer.skips.<account>`), replaced whole when the
server reports them. See [`kingdom-transfer.md`](kingdom-transfer.md).

## V27: alliance chat

`alliance_chat`: one row per alliance message the account's session heard (`player_id`, `player_name`, `text`,
`sent_at_ms`, `received_at_ms`). The server sends no clock, only the message's age, so `sent_at_ms` is the arrival
time minus that age. The newest 2,000 rows per account are kept; a message the same sender sent within five seconds of
an existing identical row is not stored twice. See [`chat.md`](chat.md).

## Size

For one account running one troop type flat out:

| Data | Rows | Order of magnitude |
| --- | ---: | --- |
| Raw stock samples | at most 1,440 a day, 72 h kept | about 4,300 rows |
| Hourly stock | 24 a day | about 8,800 a year |
| `event_log` | a handful per session | a few hundred over 90 days |
| `event_hourly` | up to ~10 kinds x 24 a day | about 90,000 a year at the very most |
| Ledger extras | 4 values on rows that already exist | none |

## Reading it

```sql
-- Crossbow stock over the last day, hourly
SELECT datetime(hour_ms/1000,'unixepoch','localtime') AS hour, min_home, last_home, last_out
FROM castle_stock_hourly
WHERE account_id='ventrilo' AND unit_id=607 ORDER BY hour_ms DESC LIMIT 24;

-- Average troops lost per attack, per hour
SELECT strftime('%m-%d %H', sent_at_ms/1000,'unixepoch','localtime') AS hour,
       count(*) AS attacks, round(avg(troops_lost),1) AS avg_lost
FROM attack_ledger
WHERE account_id='ventrilo' AND troops_lost IS NOT NULL GROUP BY 1 ORDER BY 1 DESC;

-- Why did it stop talking to the game?
SELECT datetime(at_ms/1000,'unixepoch','localtime'), kind, detail
FROM event_log WHERE account_id='ventrilo' ORDER BY id DESC LIMIT 30;

-- How often were attacks refused or delayed, per hour?
SELECT datetime(hour_ms/1000,'unixepoch','localtime') AS hour, kind, count
FROM event_hourly WHERE account_id='ventrilo' ORDER BY hour_ms DESC, kind LIMIT 50;
```

The stock figure is also shown live: after every attack-details reply the status
line ends with `unit 607: 21,459 home, 853 out, draining 1,380/h, about 15.5h left`
(and `LOW, recruit now` under three hours). The drain is the net change in home plus
out over the last hour of samples, so troops merely on a march do not count as lost
and recruiting offsets it. It needs ten minutes of samples before it claims a rate.

## What is deliberately not stored

* Credentials. The outbound `lli` packet is never recorded.
* The full inventory. Only units the running attack uses.
* Raw packets beyond the 200-row window.

## Changing the schema

Append a new migration to `MIGRATIONS` and bump `SCHEMA_VERSION`; never edit a
released one. Statements must be safe on an already-populated database: `IF NOT
EXISTS`, or an `ALTER TABLE ... ADD COLUMN` (a duplicate-column error is treated as
"already applied"). New history tables need an `account_id`, a stated retention,
and an entry in `DATA_TABLES` so the storage report and wipe see them.
