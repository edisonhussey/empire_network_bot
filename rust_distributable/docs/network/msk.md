# `msk`: Minute Skip (kingdom transfer)

| | |
| --- | --- |
| Direction | client to server, with a reply |
| Evidence | proven live (two `MS5` skips landed a 7200 s transfer, 2026-10-17) |
| Code | `empire-core/src/transfer.rs` (`msk_packet`, `skip_item`), `empire-daemon/src/direct/transfer.rs` |

## Purpose

Spends one **minute-skip item** to shorten a transfer travelling between kingdoms. It does not touch map marches or tower cooldowns; the packet carries no march or tower id.

## Request

```
%xt%EmpireEx_21%msk%1%{"MST":"MS5","KID":"3","TT":"1"}%
```

| Field | Meaning |
| --- | --- |
| `MST` | the skip item: `MS` plus its 1-based position in the seven minute-skips (below) |
| `KID` | the **target** kingdom of the transfer being skipped, as a **string** |
| `TT` | transfer type, as a **string**: `"1"` troops, `"2"` goods (read from the client; only `"1"` sent) |

| `MST` | Duration |
| --- | ---: |
| `MS1` | 1 min |
| `MS2` | 5 min |
| `MS3` | 10 min |
| `MS4` | 30 min |
| `MS5` | 1 h (proven) |
| `MS6` | 5 h |
| `MS7` | 24 h |

Only `MS5` has been sent. The list is the items file's `currencyMinutesSkipValues` (ids 1001 to 1007) and the keys the server itself lists in `gbd.sce` (`MS1`..`MS7`; an item is missing when the account has none).

## Reply

Three independent things:

1. [`sce`](sce.md): `[["MS5",995]]`, the item's new count.
2. the `msk` reply: `{"kpi":{... "UT":[{"KID":3,"RS":3589,...}]}}`, the transfer with its new seconds left.
3. on landing, a plain [`kpi`](kpi.md) with **no `UT`** for that kingdom.

## Rules and behaviour

* One `MS5` takes 3600 s off. A skip worth more than what is left finishes the transfer and the rest is lost (the second `MS5` was used with 3597 s left).
* A skip needs a transfer in progress for that `KID` and `TT`; send it after the `kut` reply shows the `UT` entry.
* The full-skip alternative `kst` (`{"KID":3,"TT":1}`, spends rubies) exists in the client; it was deliberately not used and is undocumented beyond its name.

## Handling in the code

One `msk` per paced pause while the stored `kpi` still shows the transfer travelling; stops when the item runs out, after one more than the journey needs, or when a `kpi` shows no `UT`; a refused skip is never retried ([`../architecture/kingdom-transfer.md`](../architecture/kingdom-transfer.md)).

## Captures

[`msk_request.txt`](captures/msk_request.txt), [`msk_response_first.txt`](captures/msk_response_first.txt), [`msk_response_landed.txt`](captures/msk_response_landed.txt), [`sce_push.txt`](captures/sce_push.txt).

## Open questions

Skip items other than `MS5`; `TT` `"2"`; `kst`; the error statuses.
