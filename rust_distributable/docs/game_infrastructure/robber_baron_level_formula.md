# Robber baron (dungeon) level formula

How the client turns a robber baron castle's **victory count** into the level and the "N attacks to level
up" text it shows. Read from the game's own code, then checked against in-game readings, so it is exact
for every level and kingdom (not a fitted table). Found with the proxy_packet_monitor "Download login
data" capture; the sources are listed at the end.

## Where the count comes from

The map reply `gaa` carries each robber baron castle as an `AI` entry of **type 2**:

```
[2, X, Y, ownerId, victoryCount, cooldownSeconds, KID]
 0  1  2  3        4             5                6
```

| Index | Field | Meaning |
| --- | --- | --- |
| 0 | type | `2` = robber baron castle (type 1 castle, 4 outpost) |
| 1, 2 | X, Y | map position |
| 3 | owner | `-1` (no player) |
| **4** | **victoryCount** | attacks won against it since it was last reset. `0` = untouched |
| 5 | cooldown | seconds from now until the attack cooldown ends; **negative = already over**; `-1` with count 0 = never hit |
| 6 | KID | kingdom id (the offset below depends on it) |

The level is **not sent**. The client computes it from `victoryCount` and `KID`. The same entry shape appears
inside `adi` (`gaa.AI`) and `sdi`. The client code that parses it: `_victoryCount = e[4]`,
`_dungeonLevel = DungeonConst.getLevel(victoryCount, kingdomID)`, `cooldownEnd = now + e[5]`, `kingdomID = e[6]`.

## The formula

```
FACTOR = 1.9
POWER  = 0.555

level(count, kid)        = floor(FACTOR * |count| ^ POWER) + offset(kid)
victories(level, kid)    = floor( ((level - offset(kid) + 1) / FACTOR) ^ (1 / POWER) )
attacks_to_next(count)   = victories(level, kid) + 1 - count
```

`victories(level)` is the count at which the *current* level starts, so the tooltip is
`victories(level) + 1 - count`: how many more attacks until the level changes (the client wording is
"dungeonToolTip_LevelUp_Plural / Singular"). At the maximum level for the kingdom the client shows
"max level" instead (`dungeonData.getMaxLevel(kid)`).

Rounding is `floor` in both directions. Do the math in `f64`; the inverse only floors, it does not round.

## Kingdom offset

`DungeonConst.getKingdomOffset(kid)`:

| KID | Items-file name | In-game kingdom | Offset |
| --- | --- | --- | --- |
| 0 | Classic | Great Empire (green) | **1** |
| 2 | Icecream | Everwinter Glacier | **20** |
| 1 | Dessert | Burning Sands | **35** |
| 3 | Volcano | Fire Peaks | **45** |
| 4 | Eiland | Storm Islands | 0 |
| 10 | Faction | Faction | 0 |
| anything else | | | 0 |

The in-game names are matched from the internal names (Dessert = desert, Icecream = ice), not read from a
packet; the **offsets** are straight from the code and do not depend on that matching. Only the Great
Empire (KID 0) has been checked against screen readings.

## Level start counts

Smallest `victoryCount` at which each level begins:

| Level | KID 0 (offset 1) | | Level | KID 0 (offset 1) |
| --- | --- | --- | --- | --- |
| 1 | 0 | | 10 | 17 |
| 2 | 1 | | 11 | 20 |
| 3 | 2 | | 12 | 24 |
| 4 | 3 | | 20 | 64 |
| 5 | 4 | | 40 | 232 |
| 6 | 6 | | 60 | 489 |
| 7 | 8 | | 80 | 826 |
| 8 | 11 | | | |
| 9 | 14 | | | |

Early levels take 1 attack each, then 2, then 3, and the gap keeps growing (the curve is a power law
with exponent 0.555). For another kingdom shift the level by the offset: its level `L` starts at the count
where Great Empire level `L - offset + 1` starts.

## Example data (Great Empire, KID 0)

Captured 2026-10-14, read against the in-game castle info. "You saw" is what the screen showed.

| Castle | `AI` entry | Computed | In game |
| --- | --- | --- | --- |
| 640:605 | `[2,640,605,-1,0,-1,0]` | level 1, 1 to go | level 1, 1 to go |
| a count-4 castle | `[2,x,y,-1,4,...,0]` | level 5, 2 to go | level 5, 2 to go |
| a count-7 castle | `[2,x,y,-1,7,...,0]` | level 6, 1 to go | level 6, 1 to go |
| 646:634 | `[2,646,634,-1,9,-9395542,0]` | level 7, 2 to go | level 7, 2 to go |
| 647:637 | `[2,647,637,-1,11,-9368850,0]` | level 8, 3 to go | level 8, 3 to go |
| 643:635 | `[2,643,635,-1,13,-8537759,0]` | level 8, 1 to go | level 8, 1 to go |
| 644:639 | `[2,644,639,-1,14,-9326003,0]` | level 9, 3 to go | level 9, 3 to go |

All seven match. Counts 1 to 3 and 5, 6, 8, 10, 12 were not read on screen; they follow from the same
formula (counts 0..19 give levels 1,2,3,4,5,5,6,6,7,7,7,8,8,8,9,9,9,10,10,10).

Reference implementation (Python, matches the client):

```python
import math
OFFSET = {0: 1, 2: 20, 1: 35, 3: 45}          # any other KID: 0
def level(count, kid):  return math.floor(1.9 * abs(count) ** 0.555) + OFFSET.get(kid, 0)
def victories(lv, kid): return math.floor(((lv - OFFSET.get(kid, 0) + 1) / 1.9) ** (1 / 0.555))
def to_next(count, kid): return victories(level(count, kid), kid) + 1 - count
```

## The cooldown field (index 5)

`cooldownSeconds` is relative to the moment of the reply, so `T0 = reply_time + cooldownSeconds` is a
fixed timestamp that stays the same across replies (checked across two sessions 335,756 s apart). Negative
means the cooldown ended that many seconds ago. The client constant is `DungeonConst.COOLDOWN = 10800`
(3 hours). A castle that has not been hit shows `-1`. It does not reset the count on its own in the data
seen so far: a castle sat at count 2 with a cooldown long over until it was attacked again (then 3).

## In the code

* `empire-core/src/dungeon.rs`: `level_for`, `hits_to_next`, `progress`, `kingdom_offset` (the only
  place the constants live). Its tests are the readings above.
* `account::rbc_targets` and `hunt::rbc_targets_from_map` give every type-2 row a level through it.
* `rbc_target.victory_count / level / hits_to_next` (migration V25) keep the three together; a
  looted return adds a win without a scan. See [`database.md`](../architecture/database.md).

## Not the same thing

- The `dungeons` list in the items file (`items_v<version>.json`) gives the **defenders** for each victory
  count (`countVictories`, `lordID`, `unitsL/M/R/K`, `toolL/M/R`, `skipCosts`). It does not give levels.
- `getGuards(count, kid)` (number of extra guards) is derived from the level:
  `max(0, min(50, round(0.06*(n-4)^2 + 0.5*(n-4))))` where `n = level`.
- Other item-file sections that look related but are separate systems: `bossdungeons`, `nomadCamps`,
  `samuraiCamps`, `factioninvasioncamps`, `allianceBattleGroundDungeons` (these have their own
  `dungeonlevel` fields).

## Sources

- Game code: `dll/ggs.dll.<hash>.js`, class `DungeonConst` (`getLevel`, `getVictories`,
  `getKingdomOffset`, `FACTOR_VICTORY_LEVEL = 1.9`, `POWER_VICTORY_LEVEL = 0.555`, `COOLDOWN = 10800`);
  `Game.bundle.<hash>.js`, `DungeonMapobjectVO` (the `e[4]`, `e[5]`, `e[6]` parse) and
  `DungeonMapobject.textLine2/textLine3` (the level and "attacks to go" text). Captured from
  `empire-html5.goodgamestudios.com/default/` at items version 787.03.
- Captured with proxy_packet_monitor, "Download login data". Files under
  `proxy_packet_monitor/web_data/login_data/files/` (gitignored), index in `index.jsonl`.
- Packet data: latest KID 0 session in `proxy_packet_monitor/data/sessions/` (events `GAA` seq 22, `ADI` seq 34).
- The constants are part of the client build. If a future game update changes the level curve, recapture
  with "Download login data" and re-read `DungeonConst` in `ggs.dll.*.js`.
