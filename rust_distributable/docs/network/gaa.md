# `gaa`: Game Area Activity (map viewport)

| | |
| --- | --- |
| Direction | client to server, with a reply |
| Evidence | captured, and used live by the bot |
| Code | `empire-core/src/account.rs` (`rbc_targets`, `fortress_targets`), `empire-core/src/hunt.rs` (`gaa_packet`), `empire-daemon/src/direct/observe.rs` |

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
