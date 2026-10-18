# `cds`: Commit a Support march

| | |
| --- | --- |
| Direction | client to server, with a reply |
| Evidence | captured |
| Code | `empire-core/src/station.rs` (`cds_packet`, `parse_cds`), `empire-daemon/src/direct/station.rs` |

## Purpose

Sends support troops from a castle to a destination.

## Request

```
%xt%EmpireEx_49%cds%1%{"SID":437391,"TX":468,"TY":507,"LID":0,"WT":1,"HBW":-1,"BPC":0,"PTT":0,"SD":0,"A":[[602,5]]}%
```

| Field | Meaning |
| --- | --- |
| `SID` | the source **castle id** (matches `sdi`'s `SCID`), not a movement id |
| `TX`, `TY` | destination coordinates |
| `LID` | the selected server lord/commander id |
| `A` | `[[troop_id, count], ...]` (here troop 602, five of them) |
| `WT`, `HBW`, `BPC`, `PTT`, `SD` | `1`, `-1` (no horse, nothing), `0`, `0`, `0` in the capture; their general values need more captures |

## Reply

The success reply carries `A.M.MID` (the **support** march id), `PT`, `TT`, `KID`, the source `SA`, the destination `TA`, and the troop array `A.A`. It also carries `O[]` player details like `gam`: match players by id, not by the positions of `O[0]` and `O[1]`.

## Rules and behaviour

* Match the reply's source area id and destination coordinates to the pending request before confirming it. A request injection or a timeout alone is not confirmation; a timed-out send may have succeeded, so reconcile before retrying.
* The support march's `MID` is the one [`mcm`](mcm.md) must name, not the attack's.

## Handling in the code

Step 3 of [`sequences/support.md`](sequences/support.md).

## Captures

[`cds_request.txt`](captures/cds_request.txt), [`cds_response.txt`](captures/cds_response.txt).

## Open questions

The allowed `WT`, `BPC`, `PTT`, `SD` values; what a valid destination must be (the capture sent to another player's castle in the same alliance); refusal statuses.
