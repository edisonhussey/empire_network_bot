# `kut`: Kingdom Unit Transfer

| | |
| --- | --- |
| Direction | client to server, with a reply |
| Evidence | proven live (2026-10-17: 1 unit 277 Sands to green; 20 Travelling Knights green to Fire Peaks) |
| Code | `empire-core/src/transfer.rs` (`kut_packet`, `parse_kut_reply`, `check`), `empire-daemon/src/direct/transfer.rs` |

## Purpose

Moves troops from one of the account's castles to another kingdom. Rules and price: [`../game_infrastructure/kingdom_transfer_rules.md`](../game_infrastructure/kingdom_transfer_rules.md); the whole flow: [`sequences/kingdom_transfer.md`](sequences/kingdom_transfer.md).

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

The client builds the fields in that order; `kut_packet` keeps it.

## Reply

```
%xt%kut%1%0%{"kpi":{"UL":[...],"UT":[{"KID":3,"RS":7200,"I":[[655,20]]}]}}%
```

Status `0` is success; the payload's `kpi` shows the transfer in `UT` (`KID` the target, `RS` seconds left, `I` the units). Any other status is an error dialog in the client; none has been collected for this command. A [`gcu`](gcu.md) with the cost taken off arrives alongside.

## Rules and behaviour

* **Price:** `coins = round(sum(kingdomTravellingCost * count) * unitTravelTaxRate(target) / 100)`; the journey is 7200 s into every kingdom. Live: 20 knights into Fire Peaks charged exactly 2,500.
* **One transfer per target kingdom** is shown at a time (`kpi.UT` has one entry per kingdom), so a second to the same kingdom is refused by the code.
* A `kut` with no reply is **never sent twice**: read [`kpi`](kpi.md) to learn whether it went out.
* The source castle's stock drops at once; a reading older than the send is not trusted.

## Handling in the code

`transfer::check` refuses what the server would refuse (stock, coins, locked kingdom, one already travelling, bot running); the session sends it only while the bot is off ([`../architecture/kingdom-transfer.md`](../architecture/kingdom-transfer.md)).

## Captures

[`kut_request.txt`](captures/kut_request.txt), [`kut_response.txt`](captures/kut_response.txt), [`gcu_push.txt`](captures/gcu_push.txt).

## Open questions

The error statuses; `CID` other than `-1`; whether a second transfer into a kingdom already receiving one is allowed.
