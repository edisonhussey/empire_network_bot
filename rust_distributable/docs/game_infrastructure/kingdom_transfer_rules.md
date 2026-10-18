# Kingdom transfer rules

How moving troops between kingdoms is priced and timed, where the numbers come from, and the live run that
proved them. Packets: [`../network/kut.md`](../network/kut.md), [`../network/msk.md`](../network/msk.md),
[`../network/kpi.md`](../network/kpi.md).

## Where the numbers come from

The items file (`items_v787.03.json`, the game's own data, version 787.03) and the client code. Both are
saved by `proxy_packet_monitor` ("Download login data"). `utils/scripts/build_tables.py` turns the items file into
the lookup tables (`data/tables/units.csv`, `kingdoms.csv`, `minute_skips.csv`) and the Rust that mirrors them
(`crates/empire-game/src/generated/units.rs`, `rules.rs`); run it again after a game update and the numbers follow
([`../game_data/tables.md`](../game_data/tables.md)).

## The formula

```
coins = round( sum over units of kingdomTravellingCost(unit) * count  *  unitTravelTaxRate(target) / 100 )
```

* `kingdomTravellingCost` is a per-unit field (344 units have one: 277 costs 300, 655 costs 250). A unit
  without one cannot be priced and is not offered.
* `unitTravelTaxRate` belongs to the **target** kingdom:

| Target | Items name | Tax | Journey |
| --- | --- | ---: | ---: |
| 0 Great Empire (green) | Classic | 70 % | 7200 s |
| 1 Burning Sands | Dessert | 40 % | 7200 s |
| 2 Everwinter | Icecream | 30 % | 7200 s |
| 3 Fire Peaks | Volcano | 50 % | 7200 s |
| 4 Storm Islands | Eiland | 40 % | 7200 s |

The client then applies a discount function (`getFinalCostsC1`) that was 0 in the live runs. Tools use a
flat constant instead of `kingdomTravellingCost`; they are not sent from here.

## Skips

Seven minute-skip items, keys `MS1` to `MS7`: 1, 5, 10, 30, 60, 300 and 1440 minutes (items file ids 1001 to
1007). Each `msk` spends one. A 7200 s journey takes two `MS5`. Only `MS5` has been proven.

## The live proof (2026-10-17, account `ventrilo`, `EmpireEx_21`)

Injected through `proxy_packet_monitor`'s Inject box and read back from the recorded session. Never more than
two errors in ten minutes; there were none.

| Step | Sent | Reply |
| --- | --- | --- |
| read | `kpi`, `dcl` | 20 knights (655) in green castle 16011862, Fire Peaks castle 16681051, no `UT` |
| send | `kut` `{"SCID":16011862,"SKID":0,"TKID":3,"CID":-1,"A":[[655,20]]}` | status 0; `UT` kingdom 3, `RS` 7200; `gcu` 2147483437 to 2147480937 (**-2,500**) |
| skip | `msk` `MS5` | `sce` `MS5` 996; `UT` `RS` 3589 |
| skip | `msk` `MS5` | `sce` `MS5` 995; `kpi` with no `UT` |
| confirm | `dcl` | castle 16681051 holds `[655,20]`, castle 16011862 none |

Earlier the same day: one unit 277 from Burning Sands to green cost 210 (300 x 70 %).

## Not known

* The error statuses `kut` and `msk` return, and what each means.
* Whether a second transfer to a kingdom already being travelled to is allowed (the client looks transfers up
  by target kingdom, so the code refuses it).
* `CID` other than `-1`, `TT` `"2"` (goods) and `kst`: read from the client, never sent.
* Skip items other than `MS5`.
