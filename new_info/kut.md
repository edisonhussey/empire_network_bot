# `kut`: send troops to another kingdom

The real "transfer troops between kingdoms" request (the `map_of_kingdom` / transfer-troops dialog).
Client code: `C2SKingdomUnitTransferVO` and `CastleTransferTroopsDialog`; session `34d3316a`.

```
%xt%EmpireEx_21%kut%1%{"SCID":16366514,"SKID":1,"TKID":0,"CID":-1,"A":[[277,1]]}%
```

| Field | Meaning | Where to get it |
| --- | --- | --- |
| `SCID` | **source castle id** (the `AID` of the castle the troops leave) | `dcl` `C[].AI[].AID` |
| `SKID` | source kingdom of that castle | `dcl` `C[].KID` |
| `TKID` | target kingdom | kingdom ids below |
| `CID` | target castle id; `-1` = no particular castle (the client sends `-1` unless a castle is picked in the dialog) | `-1` verified into kingdom 0 |
| `A` | `[[unit_id, count], ...]` | `dcl` `AC` of the **source** castle |

Kingdom ids: 0 Great Empire (green), 1 Burning Sands, 2 Everwinter, 3 Fire Peaks, 4 Storm Islands
(see `docs/game_infrastructure/robber_baron_level_formula.md` in `rust_distributable` for the names).
The earlier note here said "kid 2 (sands)"; the packet itself says `SKID` 1, and the account's castle
`16366514` is in kingdom 1 in the `dcl`.

## Cost (verified against the coin counter)

```
coins = round( sum over units of kingdomTravellingCost(unit) * unitTravelTaxRate(target kingdom) / 100 * count )
```

`kingdomTravellingCost` is a field on each unit in the items file; the tax rate belongs to the **target**
kingdom (`kingdoms` in the items file: green 70, Everwinter 30, Burning Sands 40, Fire Peaks 50,
Storm Islands 40). Check: unit 277 costs 300, green tax 70 %, so 1 unit = 210 coins, and the `gcu` after
the send went from 2147483647 to 2147483437 (210 less). The client applies a further discount function
(`getFinalCostsC1`) that was 0 here. Tools use a flat constant instead (`TravelConst`), not checked.
Travel time is `unitTravelTime`, 7200 s into every kingdom.

## Reply

`%xt%kut%1%<status>%{...}%`. Status `0` is success; any other status is an error dialog in the client
(the code for each reason is not collected yet). On success the payload carries `kpi`, which the client
reads together with `gcu` and `gui` if present:

```
%xt%kut%1%0%{"kpi":{"UL":[ ...unchanged... ],"UT":[{"KID":0,"RS":7200,"I":[[277,1]]}]}}%
```

`UT` = unit transfers in flight, keyed by **target** kingdom: `KID` target, `RS` seconds remaining,
`I` the units on the way. A `gcu` (new coin total) is pushed alongside and is handled on its own;
do not wait for it.

## Needed before sending

1. A fresh `dcl` for the source castle (`SCID`), to know the unit is really there and how many.
2. The target kingdom unlocked (`kpi.UL[KID].U` is `1`).
3. Enough coins for the cost above.
4. No transfer already travelling to that target kingdom (`UT`).

Step by step example for 20 Traveling Knights green to Fire Peaks: [`transfer_troops_poc.md`](transfer_troops_poc.md).
