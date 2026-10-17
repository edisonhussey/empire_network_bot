# Proof of concept: 20 Traveling Knights, green to Fire Peaks main castle, with skips

Nothing here has been sent by us. These are the exact lines to paste into the Inject box (direction
**To server**, one at a time) in `proxy_packet_monitor`. The pieces are built from the `dcl` in session
`34d3316a` and the proven `kut` and `msk` packets in the same session. Background:
[`kut.md`](kut.md), [`msk_info.md`](msk_info.md), [`kpi.md`](kpi.md), [`state_funnels.md`](state_funnels.md).

## The values

| What | Value | Source |
| --- | --- | --- |
| XT header | `EmpireEx_21` | every packet in the two latest sessions. The Inject box hint shows `EmpireEx_19`; that is only an example, use **21** for this account |
| Unit | `655` "Travelling Knights" | items file `units`; `dcl` shows `[655,20]` in the green castle, so exactly the 20 |
| Source castle `SCID` | `16011862` | the green (`KID` 0) castle holding the 655s, from `dcl`. The other two green castles hold none |
| Source kingdom `SKID` | `0` | green |
| Target kingdom `TKID` | `3` | Fire Peaks |
| Target castle `CID` | `-1` | what the client sends when no castle is picked; the account has one castle in Fire Peaks, `16681051` |
| Cost | 20 x 250 x 50 % = **2,500 coins** | unit cost 250, Fire Peaks tax 50 %; same formula that matched the 210-coin check for unit 277 |
| Travel time | 7200 s (2 h) | `unitTravelTime`, so 2 x `MS5` |

## The sequence

**1. Check the starting state** (optional, 2 lines, safe to repeat):

```
%xt%EmpireEx_21%dcl%1%{"CD":0}%
%xt%EmpireEx_21%kpi%1%{}%
```

Look for: castle `16011862` has `[655,20]` (or more); `kpi` shows kingdom 3 with `"U":1` and there is no
`UT` entry for `"KID":3` yet.

**2. Send the troops:**

```
%xt%EmpireEx_21%kut%1%{"SCID":16011862,"SKID":0,"TKID":3,"CID":-1,"A":[[655,20]]}%
```

Expected: `%xt%kut%1%0%{"kpi":{...,"UT":[{"KID":3,"RS":7200,"I":[[655,20]]}]}}%`, and a `gcu` with coins
2,500 lower. Status `0` is success. **Wait for this reply before step 3**, because a skip only works on
a transfer that exists.

**3. Skip the first hour:**

```
%xt%EmpireEx_21%msk%1%{"MST":"MS5","KID":"3","TT":"1"}%
```

Expected: an `sce` `[["MS5",n]]` and an `msk` reply whose `UT` for kingdom 3 has `RS` about 3598.

**4. Skip the rest:**

```
%xt%EmpireEx_21%msk%1%{"MST":"MS5","KID":"3","TT":"1"}%
```

Expected: another `sce` (n lower by one) and a `kpi` with **no `UT`**. The knights have landed.

**5. Confirm** with the same `dcl` line as step 1: castle `16681051` should now list `[655,20]` and
`16011862` should not.

Send each line alone and wait for its reply; the Inject box can send several lines at once, but the
skip must not go out before the `kut` reply. Check the whole thing in the session viewer: all these
frames are recorded.

## What is proven and what is not

| Piece | Status |
| --- | --- |
| `kut` shape and fields, `CID` -1 | proven (unit 277, sands to green) |
| `msk` shape, `MS5`, `TT` "1", two skips finishing 7200 s | proven (same session) |
| Unit and castle ids | from the `dcl` |
| Cost 2,500 | formula verified on one unit, not at 20 or on Fire Peaks |
| Target kingdom 3 and `CID` -1 into a kingdom whose only castle is not the home | **not tested** (the proven transfer went into green) |
| Unit 655 (an event unit, `eventIDs` 23,29,30,32) allowed to travel | **not tested**. The client does not filter units; the server decides |

## If it does not work

* `kut` status not `0`: stop there and note the status number and the packet; nothing was spent.
  First retry with `"CID":16681051` (the target castle named explicitly).
* To separate "Fire Peaks or `CID`" from "unit 655", repeat with the proven unit instead, 20 of
  unit 277 (the castle holds 9,890; cost 20 x 300 x 50 % = 3,000 coins):
  `%xt%EmpireEx_21%kut%1%{"SCID":16011862,"SKID":0,"TKID":3,"CID":-1,"A":[[277,20]]}%`
* A `kut` while a transfer to the same kingdom is still travelling is untested; check `kpi` first.
* `msk` replies with no change or an error: check there is still a `UT` for `"KID":3`; it may already
  have landed.
* Skips are the only thing not recoverable: each `msk` uses one `MS5` item. The rubies skip (`kst`, see
  `msk_info.md`) is deliberately not used.

## Why it is shaped this way

The sends are separate, each decided from the state already on screen (stock, coins, `UT`), and each
confirmed by what changes (`UT` appears, then disappears), not by one routine waiting on another. That is
the design in [`state_funnels.md`](state_funnels.md) and what a bot should copy.
