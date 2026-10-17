# `kut` (Kingdom Unit Transfer)

Moves troops from one of the account's castles to another kingdom. Proven live on 2026-10-17: one unit 277
from Burning Sands to green, then 20 Travelling Knights (unit 655) from green to Fire Peaks, both with
status 0. Rules and cost: [`../game_infrastructure/kingdom_transfer_rules.md`](../game_infrastructure/kingdom_transfer_rules.md).

## Request

```
%xt%EmpireEx_21%kut%1%{"SCID":16011862,"SKID":0,"TKID":3,"CID":-1,"A":[[655,20]]}%
```

| Field | Meaning | Source |
| --- | --- | --- |
| `SCID` | the source **castle id** (`AID` in `dcl`) | `dcl` `C[].AI[].AID` |
| `SKID` | the source castle's kingdom | `dcl` `C[].KID` |
| `TKID` | the target kingdom | kingdom ids in [`kpi.md`](kpi.md) |
| `CID` | the target castle id; `-1` when none is chosen (the client sends `-1` unless a castle is picked) | `-1` proven into green and into Fire Peaks |
| `A` | `[[unit_id, count], ...]`, troops only | `dcl` `AC` of the **source** castle |

The client builds it in that field order; `empire_core::transfer::kut_packet` keeps the order.

## Reply

```
%xt%kut%1%0%{"kpi":{"UL":[...],"UT":[{"KID":3,"RS":7200,"I":[[655,20]]}]}}%
```

Status `0` is success; the payload's `kpi` shows the transfer in `UT` (`KID` the target, `RS` seconds left,
`I` the units on the way). Any other status is an error dialog in the client; the status codes seen so far
are in [`../architecture/errors.md`](../architecture/errors.md), and none has been collected for this
command yet. A `gcu` (coins) arrives alongside and is stored separately, see [`kpi.md`](kpi.md).

## What it costs and takes

`coins = round(sum(kingdomTravellingCost(unit) * count) * unitTravelTaxRate(target kingdom) / 100)`; the
journey is 7200 s into every kingdom. Live: 20 knights into Fire Peaks charged exactly 2,500 coins.

## Handling in the code

`empire-core/src/transfer.rs` builds the packet and checks everything the server would refuse (stock, coins,
unlocked kingdom, a transfer already travelling there); `direct/transfer.rs` sends it, only while the bot is
off. A `kut` with no reply is never sent twice: `kpi` is read to learn whether it went out. See
[`../architecture/kingdom-transfer.md`](../architecture/kingdom-transfer.md).
