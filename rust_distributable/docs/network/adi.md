# `adi` (Attack Details Inspection)

The `adi` packet is used to inspect a target before sending an attack. It queries the server for the target's current state, defensive setup, and capacity.

## Outbound Request

When targeting an RBC (Robber Baron Castle) or player, the client sends an `adi` packet to request the attack screen details.

**Example Request:**
```jsonc
{"KID":1,"SX":593,"SY":613,"TX":554,"TY":655}
```
* `KID`: Kingdom ID (0 = Green, 1 = Sands, 10 = Berimond, etc.)
* `SX`, `SY`: Source Coordinates (Your castle)
* `TX`, `TY`: Target Coordinates

## Inbound Response

The server responds with the target's available limits, flank capacity, and valid tools/troops. 

**Example Success Response:**
```jsonc
{
  "payload": [1,2,5,-1,[[4,58,[114.9]],[5,68,[75.4]]],-1,-1,0,-1,-1,3,[1,5,3290,[]]],
  "status": "0"
}
```

### Observed Behaviors & Edge Cases

1. **Target on Cooldown (`status: 95`)**:
   If an RBC is on cooldown (recently hit by you or another player), the server
   refuses the inspection with `status: "95"` and a `null` payload. It means the
   client's picture of the tower is out of date, not that anything is wrong.
   ```jsonc
   16:50:56  IN   adi       {"payload":null,"status":"95"}
   ```
   The tower's real remaining cooldown is in its `gaa` row ([`gaa.md`](gaa.md)).
   How the bot reacts is in [`ARCHITECTURE.md`](../../ARCHITECTURE.md#attack-scheduling).

2. **Berimond Camps (`KID: 10`)**:
   Berimond camps do not have cooldowns. If an `adi` returns `status: 95` for a Berimond camp, it means the camp was defeated by another player right before we inspected it. The target should be permanently deleted from the database.

3. **`gli.C` is the whole roster, not availability.**
   Every `adi` reply lists all owned commanders (`gli.C[].ID`, e.g. all 35)
   regardless of who is marching. It cannot be intersected with local state to
   find free commanders. Commander availability comes only from our own state:
   `cra` ack (`TT`) then `cat` (return time).
