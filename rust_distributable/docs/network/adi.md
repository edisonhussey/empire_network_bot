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
   If an RBC is on cooldown (recently hit by you or another player in an instanced kingdom like Sands), the server will protect the target. It returns `status: "95"` and a `null` payload.
   ```jsonc
   16:50:56  IN   adi       {"payload":null,"status":"95"}
   ```
   **Handling Rule:** A `95` is not a fatal operational fault; it is the server gracefully protecting a cooldown. The bot must parse this as "Target Unavailable", defer the target locally for its expected cooldown duration (e.g., 1 hour), and seamlessly transition to the next valid coordinate.

2. **Berimond Camps (`KID: 10`)**:
   Berimond camps do not have cooldowns. If an `adi` returns `status: 95` for a Berimond camp, it means the camp was defeated by another player right before we inspected it. The target should be permanently deleted from the database.
