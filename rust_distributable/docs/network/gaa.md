# `gaa` (Game Area Activity)

The `gaa` packet is the core map scanning viewport request. It pulls down the live state of the map grid.

## Outbound Request

The client requests a rectangular bounding box of coordinates to inspect.

**Example Request:**
```jsonc
{"KID":1,"AX1":572,"AY1":598,"AX2":584,"AY2":610}
```
* `KID`: Kingdom ID (0 = Green, 1 = Sands, 10 = Berimond)
* `AX1`, `AY1`: Top-left bounds
* `AX2`, `AY2`: Bottom-right bounds

### Viewport Origins
The 3x2 tile grid always steps 13 tiles, but the offset from the main castle is kingdom-specific and cannot be collapsed into a single formula.
* Green (`KID: 0`): Offset is `-13, -13`
* Sands (`KID: 1`): Offset is `-21, -15`

## Inbound Response

The server replies with a dense payload of all entities within the bounding box, including their area types, levels, and owner IDs.

**Example Entities in Payload:**
* `Area Type 2`: Standard Robber Baron Castle (RBC)
* `Area Type 17`: Berimond Camp (Kingdom 10)
* `Area Type 12`: Player Castle (Sands)

### RBC rows carry the server's cooldown

A tower row is `[2, x, y, -1, level_raw, remaining_s, 1]`. `remaining_s` is the
seconds until the server lets the tower be attacked again, and `-1` (or any
non-positive value) means it is ready. Levels in Sands are `level_raw` mapped
through `sands_level` (raw `112` is level 61).

```jsonc
{"AI":[[2,584,626,-1,112,9504,1]]}   // cooling for 9504 s
{"AI":[[2,600,628,-1,112,-1,1]]}     // ready
```

Fortress rows (`type 11`) use the same slot for their remaining seconds.
Berimond camps (`type 17`) report no cooldown.

### Observed behaviours

1. **Unsolicited tile updates.** The server pushes a one-row `gaa` for a tower
   around the time its army comes home (`cat`). It is a free, authoritative
   cooldown read.
2. **Cooldowns are not static.** Other players hit the same towers, so the map
   read at login goes stale. The remaining value is the only way to know about
   those hits without being refused (see [`errors.md`](../architecture/errors.md), status 95).
3. **A tile read is cheap.** A 13x13 window centred on one tower
   (`AX1 = x-6 ... AX2 = x+6`) returns that tower and its neighbours.
