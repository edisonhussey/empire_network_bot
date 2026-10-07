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
