# `gaa`: Game Area Activity (map viewport)

| | |
| --- | --- |
| Direction | client to server, with a reply |
| Evidence | captured, and used live by the bot |
| Code | `empire-core/src/storm.rs` (`forts_in_gaa`, Storm Islands), `empire-core/src/account.rs` (`rbc_targets`, `fortress_targets`), `empire-core/src/hunt.rs` (`gaa_packet`), `empire-daemon/src/direct/observe.rs` |

## Purpose

Reads the live state of a rectangle of the map: towers, fortresses, camps and castles, with their levels and cooldowns.

## Request

```jsonc
{"KID":1,"AX1":572,"AY1":598,"AX2":584,"AY2":610}
```

| Field | Meaning |
| --- | --- |
| `KID` | kingdom id (0 green, 1 Sands, 10 Berimond) |
| `AX1`, `AY1` | top-left bounds |
| `AX2`, `AY2` | bottom-right bounds |

The 3x2 tile grid always steps 13 tiles, but the offset from the main castle is kingdom-specific: green is `-13, -13`, Sands `-21, -15`.

## Reply

A dense payload of everything inside the box, `AI[]` rows by area type: type 2 a standard tower, 17 a Berimond camp (kingdom 10), 12 a player castle (Sands), 11 a fortress.

A **tower row** is `[2, x, y, -1, victory_count, remaining_s, kingdom_id]`:

* `victory_count` is the number of attacks won against it, **not its level**. The level is worked out from it and the kingdom: [`../game_infrastructure/robber_baron_level_formula.md`](../game_infrastructure/robber_baron_level_formula.md).
* `remaining_s` is seconds until it can be attacked again; `-1` or any non-positive value means ready.

```jsonc
{"AI":[[2,584,626,-1,112,9504,1]]}   // cooling for 9504 s
{"AI":[[2,600,628,-1,112,-1,1]]}     // ready
```

Fortress rows (type 11) use the same slot for their remaining seconds. Berimond camps (type 17) report no cooldown.

## Rules and behaviour

1. **Unsolicited tile updates.** The server pushes a one-row `gaa` for a tower around the time its army comes home (`cat`): a free, authoritative cooldown read.
2. **Cooldowns are not static.** Other players hit the same towers, so the map read at login goes stale. The remaining value is the only way to know about those hits without being refused ([`../architecture/errors.md`](../architecture/errors.md), status 95).
3. **A tile read is cheap.** A 13x13 window centred on one tower (`AX1 = x-6 ... AX2 = x+6`) returns that tower and its neighbours.

## Handling in the code

Every map response refreshes known towers' cooldowns; scans store new ones with their level (`rbc_target`, [`../architecture/database.md`](../architecture/database.md) V25).

## Captures

None saved as a file; see [`adi.md`](adi.md) for refusing a tower that is cooling.

## Open questions

Elements of the tower row beyond the ones listed.


## Storm Islands (`KID` 4)

Captured 2026-11-16 (us1, the browser opening the storm map, then attacking a level-60 fort). Windows are the usual
13 x 13; the client read the castle's window first (`546:520`, castle at `557:531`) and the ring around it.

Rows seen: type 12 (player castles, the account's own storm castle among them), 24 (storm objects owned by `-403`), 25
(storm forts, owner `-1`) and 31 (`[31, x, y]`, decoration). A storm fort:

```
[25, 551, 542, 4, -1, 12, 0, 8, 0]
 type x    y    ?  owner level-code cooldown ? cooldown
```

* Level code `AI[5]`: 7 or 12 = level 60, 8 or 13 = 70, 9 or 14 = 80, 10 = 40, 11 = 50 (from the earlier Python storm bot;
  the captured attack on a code-12 fort was the level-60 fort the person chose).
* Free only when `AI[6]` and `AI[8]` are both 0; a fort on cooldown carries the same number of seconds in both
  (`[25,546,527,4,-1,10,34300,0,34300]`).
* The `adi` reply for a fort carries its current row as `gaa.AI` (one row, not a list), so a fort taken since the scan is
  visible before the attack: `{"KID":4,"HAWL":4,"SCID":4960,"gaa":{"AI":[25,551,542,4,-1,12,0,8,0]},...}`.

The `cra` itself is the usual one with `KID` 4 and the storm castle as `SX`/`SY`. How the bot uses this:
[`../architecture/storm-islands.md`](../architecture/storm-islands.md).
