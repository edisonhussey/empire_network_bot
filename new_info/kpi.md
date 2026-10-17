# `kpi`: kingdom state (the full picture)

The server's whole view of the kingdoms: which are unlocked, and what is travelling between them.
Request and reply (also pushed unasked, and embedded in the replies of `kut` and `msk`):

```
%xt%EmpireEx_21%kpi%1%{}%
%xt%kpi%1%0%{"UL":[{"KID":0,"U":1,"C":0},{"KID":1,"U":1,"C":1,"SL":4},{"KID":2,"U":1,"C":1,"SL":1,"SPW":0,"SPS":0,"SPF":0,"SPC1":0},{"KID":3,"U":1,"C":1,"SL":1,"SPW":81000,"SPS":81000,"SPF":0,"SPC1":120000},{"KID":4,"KRS":1282599,"CRS":2,"U":0,"C":0}]}%
```

## `UL[]`: one row per kingdom (client `CastleKingdomVO.parseUnlockInfo`)

| Field | Meaning |
| --- | --- |
| `KID` | kingdom id |
| `U` | `1` = unlocked for this account (kingdom 4 here is `0`) |
| `C` | `1` = "has a Kontor" (`_hasContor`), the kingdom's trading post. Green shows `0` |
| `SL` | slum level (the outskirts upgrade level, up to 4) |
| `SPW`, `SPS`, `SPF`, `SPC1` | the goods (wood, stone, food, coins) already paid toward the **next** slum level |
| `KRS` | seconds until the kingdom resets (kingdom 4, Storm Islands; counts down 1 per second in the capture) |
| `CRS` | reward set id (kingdom 4) |

## `UT[]` and `RT[]`: what is travelling

* `UT`: troops in transit, `{"KID": target, "RS": seconds left, "I": [[unit, count], ...]}`.
* `RT`: resources in transit (`G` goods instead of `I`), same `KID` and `RS`.
* The client **replaces** its list of transfers each time a `kpi` arrives (it clears and refills), so
  a `kpi` with no `UT` means nothing is travelling. Treat it as a full snapshot, never as a delta.

Where it appears: the `kpi` reply, the `kut` reply (`kpi` inside), the `msk` reply (`kpi` inside), and
alone shortly after a skip or landing. Handle all of them in the same place; see [`state_funnels.md`](state_funnels.md).
