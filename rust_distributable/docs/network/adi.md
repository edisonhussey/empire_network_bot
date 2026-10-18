# `adi`: Attack Details Inspection

| | |
| --- | --- |
| Direction | client to server, with a reply |
| Evidence | captured |
| Code | `empire-daemon/src/direct/automation.rs` (`ReadyAdi`, `AwaitAdi`), `empire-core/src/hunt.rs` (`AttackInfoKind`) |

## Purpose

Inspects a target before an attack: its current state, defence setup and capacity. A type-2 tower uses `adi`; a type-11 fortress uses `abi`.

## Request

```jsonc
{"KID":1,"SX":593,"SY":613,"TX":554,"TY":655}
```

| Field | Meaning |
| --- | --- |
| `KID` | kingdom id (0 green, 1 Sands, 10 Berimond, ...) |
| `SX`, `SY` | source coordinates (your castle) |
| `TX`, `TY` | target coordinates |

## Reply

The target's limits, flank capacity and valid tools and troops.

```jsonc
{"payload": [1,2,5,-1,[[4,58,[114.9]],[5,68,[75.4]]],-1,-1,0,-1,-1,3,[1,5,3290,[]]], "status": "0"}
```

## Rules and behaviour

1. **Target on cooldown (`status: 95`).** A tower on cooldown (hit by you or another player) refuses the inspection with status `95` and a `null` payload. It means the client's picture of the tower is out of date, not that anything is wrong. The tower's real remaining cooldown is in its `gaa` row ([`gaa.md`](gaa.md)). How the bot reacts: [`../architecture/attack-scheduling.md`](../architecture/attack-scheduling.md).
   ```jsonc
   16:50:56  IN   adi       {"payload":null,"status":"95"}
   ```
2. **Berimond camps (`KID` 10).** They have no cooldowns. A `95` for a camp means another player defeated it just before the inspection; delete the target for good.
3. **`gli.C` is the whole roster, not availability.** Every `adi` lists all owned commanders (`gli.C[].ID`, for example all 35) whoever is marching, so it cannot be used to find free commanders. Availability comes only from our own state: the `cra` ack (`TT`), then `cat` (return time).

## Handling in the code

Phase `ReadyAdi` sends it after a paced pause; `AwaitAdi` waits for the reply (a timeout quarantines the target). See [`../architecture/attack-scheduling.md`](../architecture/attack-scheduling.md).

## Captures

None saved as a file; the shapes above are from live logs.

## Open questions

The meaning of each element of the reply array beyond the level (`TL` row in `AE`).
