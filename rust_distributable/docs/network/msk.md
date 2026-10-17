# `msk` (Minute Skip, kingdom transfer)

Spends one **minute-skip item** to shorten a transfer travelling between kingdoms. It does not touch map
marches or tower cooldowns (the packet carries no march or tower id). Proven live on 2026-10-17: two `MS5` on
a 7200 s transfer into Fire Peaks landed it. Rules: [`../game_infrastructure/kingdom_transfer_rules.md`](../game_infrastructure/kingdom_transfer_rules.md).

## Request

```
%xt%EmpireEx_21%msk%1%{"MST":"MS5","KID":"3","TT":"1"}%
```

| Field | Meaning |
| --- | --- |
| `MST` | the skip item: `MS` plus its 1-based position in the seven minute-skips (below) |
| `KID` | the **target** kingdom of the transfer being skipped, as a **string** |
| `TT` | transfer type as a **string**: `"1"` troops, `"2"` goods. Read from the client; only `"1"` has been sent |

| `MST` | Duration |
| --- | ---: |
| `MS1` | 1 min |
| `MS2` | 5 min |
| `MS3` | 10 min |
| `MS4` | 30 min |
| `MS5` | 1 h (proven) |
| `MS6` | 5 h |
| `MS7` | 24 h |

Only `MS5` has been sent; the others come from the items file (`currencyMinutesSkipValues`, ids 1001 to
1007) and the keys the server itself lists in `gbd.sce` (`MS1`..`MS7`, an item missing when the account has
none).

## Replies

Each skip answers with three independent things:

1. `sce`: `[["MS5",995]]`, the item's new count.
2. the `msk` reply: `{"kpi":{... "UT":[{"KID":3,"RS":3589,...}]}}`, the transfer with its new seconds left.
3. when it lands, a plain `kpi` with **no `UT`** for that kingdom.

Live timeline (a 7200 s transfer, skips about 10 s apart): 7200, then 3589 after one `MS5`, then landed after
the second. One `MS5` takes off 3600 s; a skip worth more than what is left finishes it and the rest is lost.

## Handling in the code

`direct/transfer.rs` spends the chosen item one at a time, each after a paced pause and only while a transfer
is really travelling (read from the stored `kpi`), stops when the item runs out or after one more than the
journey needs, and never retries a refused skip. The window offers only skip items a journey can use in ten
or fewer. See [`../architecture/kingdom-transfer.md`](../architecture/kingdom-transfer.md).

The full-skip alternative `kst` (`{"KID":3,"TT":1}`, rubies) exists in the client and was deliberately not
used; it is undocumented beyond its name.
