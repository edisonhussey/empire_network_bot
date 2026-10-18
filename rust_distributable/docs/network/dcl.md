# `dcl`: Castle List (every castle and the units in it)

| | |
| --- | --- |
| Direction | client to server, with a reply |
| Evidence | proven live (`CD` 1 in 2026-09 captures, `CD` 0 on 2026-10-17) |
| Code | `empire-core/src/station.rs` (`dcl_packet`, `parse_dcl`), `store/station.rs` (`castle_inventory`), `empire-daemon/src/direct/inventory.rs` |

## Purpose

The overview of every castle the account owns, with the troops and tools in each. It is the "read before you build a packet" step for anything that moves units: support ([`cds`](cds.md)) and kingdom transfers ([`kut`](kut.md)).

## Request

```
%xt%EmpireEx_21%dcl%1%{"CD":0}%
```

`CD` is `1` in the older captures and `0` in the 2026-10-17 ones, and both return the full list; what it selects is not known. (The same structure also arrives inside `gbd` at login.)

## Reply

```jsonc
{"PID":16862926,"C":[{"KID":0,"AI":[{"AID":16011862,"W":804000.0,...,"AC":[[655,20],[277,9890],...]}, ...]}, ...]}
```

| Field | Meaning |
| --- | --- |
| `PID` | the **player** id |
| `C[].KID` | kingdom |
| `C[].AI[].AID` | the **castle** id, not the account. It is the `SCID` of `sdi`/`kut` and the `SID` of `cds` |
| `AI[].AC` | `[[item_id, count], ...]` for that castle only: troops **and** tools mixed, not labelled |
| `W S F C O G A I ...` | the castle's stock of wood, stone, food, coins and the other goods |

On the 2026-10-17 account: green (`KID` 0) has three castles (16011862 with the big army, 16632819, 16673031), kingdom 1 has 16366514, kingdom 2 has 16366513, kingdom 3 (Fire Peaks) has 16681051, kingdom 4 none.

## Rules and behaviour

* Match `C[].KID` and `AI[].AID` to the castle you mean; never apply one castle's `AC` to another.
* Filter `AC` against the troop table: `602` is a troop; `614` (scaling ladder), `660` (banner) and `637` are tools. A row with count `0` is not available inventory. Unit ids are in the items file `units` list.
* Cross-check against `sdi`'s `gui.I` before building a support `cds`.
* The auto-support proof of concept asks for `dcl` about seven seconds before the estimated landing and takes up to ten distinct troop types at their full home counts. The packets do not prove an unlimited-per-type rule or a total support capacity; a refusal is recorded, not retried unchanged.
* A castle's stock changes the moment troops leave (support, a kingdom transfer), so a reading older than the last send is not trusted ([`kut.md`](kut.md)).
* [`rue`](rue.md) updates one castle's count for one unit between readings.

## Handling in the code

The Overview's refresh asks for a `dcl` when the connection is free (`inventory.rs`); `station::on_dcl` stores it; the transfer checks read it.

## Captures

[`dcl_exchange.txt`](captures/dcl_exchange.txt) (request and one-castle reply, `EmpireEx_49`), [`dcl_request.txt`](captures/dcl_request.txt) (`CD` 0, `EmpireEx_21`).

## Open questions

What `CD` selects; the multi-castle layout is confirmed only on the 2026-10-17 account.
