# `msk`: minute-skip a kingdom transfer (the "skip" item)

Spends one **minute-skip item** to shorten a transfer that is travelling between kingdoms. It does
not touch map marches or robber baron cooldowns; the packet carries no march or tower id.

Evidence: session `34d3316a` (2026-10-17 09:13, account `ventrilo`, header `EmpireEx_21`) and the game
client (`ggs.dll` command table, `Game.bundle` `C2SMinuteSkipKingdomTransferVO` and
`KingdomUnitsTravelMinuteSkipProperties`). "Verified" below means seen on the wire; "from client" means
read from the client code but not yet sent by us.

## Request

```
%xt%EmpireEx_21%msk%1%{"MST":"MS5","KID":"0","TT":"1"}%
```

| Field | Meaning | Status |
| --- | --- | --- |
| `MST` | which skip item to spend (table below) | `MS5` verified, others inferred |
| `KID` | the **target** kingdom of the transfer being skipped (where the troops are going), sent as a **string** | verified (`"0"` for a transfer into green) |
| `TT` | transfer type: `"1"` troops, `"2"` goods/resources, sent as a string | `"1"` verified; `"2"` from client |

`KID` and `TT` are strings on the wire (the client calls `.toString()` on both). Keep them strings.

## The skip items

The items file (`items_v787.03.json`, `currencyMinutesSkipValues`) lists seven skip currencies, ids
1001 to 1007, worth 1, 5, 10, 30, 60, 300 and 1440 minutes. `MS5` is the fifth, one hour, as observed.
So the key is `MS` plus the 1-based position:

| `MST` | Item id | Duration | Seconds |
| --- | --- | --- | ---: |
| `MS1` | 1001 | 1 min | 60 |
| `MS2` | 1002 | 5 min | 300 |
| `MS3` | 1003 | 10 min | 600 |
| `MS4` | 1004 | 30 min | 1,800 |
| `MS5` | 1005 | 1 hour | 3,600 (verified) |
| `MS6` | 1006 | 5 hours | 18,000 |
| `MS7` | 1007 | 24 hours | 86,400 |

This is the "logical pattern": 1, 5, 10, 30, 60, 300, 1440 minutes. Only `MS5` has been sent; the others
follow from the table and the client's skip dialog but are unconfirmed. The account's stock of each item
is only seen in `sce` after that item is spent (not known otherwise).

## What happens (verified, two sends in a row)

| Time | Packet | Result |
| --- | --- | --- |
| 09:13:30 | `kut` reply | `UT`: kingdom 0, `RS` 7200, `[[277,1]]` |
| 09:13:32 | `msk` `MS5` | `sce` `[["MS5",998]]`; `msk` reply with `kpi.UT` `RS` **3598** |
| 09:13:33 | `msk` `MS5` again | `sce` `[["MS5",997]]`; `msk` reply and a `kpi` with **no `UT`** (landed) |

* One skip removes 3,600 s: 7200 s, 2 s elapsed, 3598 s left. A second skip with 3597 s left finished it
  even though it is worth more than what was left. The overshoot is simply lost (one whole item is spent).
* Landing is announced by the next `kpi` having no `UT` entry for that kingdom.
* Each `msk` produces three things, all independent: an `sce` (item count), the `msk` reply (full `kpi`
  state) and, at the end, a plain `kpi`. See [`state_funnels.md`](state_funnels.md).

## Planning how many to send

A transfer into any kingdom takes 7200 s (`unitTravelTime`, all kingdoms in the items file), so two
`MS5` always finish it from the start. In general, for `R` seconds left (`UT[].RS`): the number of items
needed with only `MS5` is `ceil(R / 3600)`. Best to recompute `R` from the latest `kpi` rather than
counting, and to send the next skip only after the previous `msk` reply, so a failed or refused skip is
noticed (see the POC in [`transfer_troops_poc.md`](transfer_troops_poc.md)). Combining sizes
(for example `MS6` for the bulk and a small one for the rest) is possible but untested.

## The ruby alternative: `kst` (from client only)

`C2SKingdomSkipTransferVO(KID, TT)` is the "skip everything with rubies" button:

```
%xt%EmpireEx_21%kst%1%{"KID":3,"TT":1}%
```

Not captured, so the exact field types and the price are unconfirmed (the items file lists
`skipUnitTravelC2Cost` 240 rubies for every kingdom, and the client may scale it). It spends rubies
(`C2`), so use the item skips above for testing.

## Rules worth keeping

* **One unit transfer per target kingdom at a time.** `kpi.UT` has one entry per target kingdom and the
  client looks transfers up by target kingdom. Sending a second batch to the same kingdom before the
  first lands is untested; do not rely on it. A skip applies to that kingdom's transfer as a whole.
* `msk` needs a transfer to be in progress for that `KID`/`TT`; the client only offers it while
  `remainingTimeInSeconds > 0`. Send it after the `kut` reply shows the `UT` entry.
* Successful skips change counts and times only; the real source of truth is the next `kpi`.
