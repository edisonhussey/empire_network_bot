# Sequence: kingdom transfer (`dcl`, `kpi`, `kut`, `msk`)

Moving troops to another kingdom and, optionally, skipping the journey. Proven live on 2026-10-17 by hand and
the lines below are exactly what was sent. The code that does the same: [`../../architecture/kingdom-transfer.md`](../../architecture/kingdom-transfer.md).
Prices and numbers: [`../../game_infrastructure/kingdom_transfer_rules.md`](../../game_infrastructure/kingdom_transfer_rules.md).

| Step | We send | The server answers | Used for |
| --- | --- | --- | --- |
| 1 | [`dcl`](../dcl.md) `{"CD":0}` and [`kpi`](../kpi.md) `{}` | every castle's stock; the kingdoms, with any `UT` | The castle holds the units, the kingdom is unlocked (`U`:1), nothing is travelling there |
| 2 | [`kut`](../kut.md) `{"SCID","SKID","TKID","CID":-1,"A":[[unit,count]]}` | status 0 and `kpi` with `UT` (`RS` 7200); a [`gcu`](../gcu.md) with the cost off | Sends the troops |
| 3 | [`msk`](../msk.md) `{"MST":"MS5","KID":"<target>","TT":"1"}` | [`sce`](../sce.md) (item count), `msk` reply with `UT` (`RS` less 3600) | Skips an hour |
| 4 | the same `msk` again | `sce`, and a `kpi` with **no `UT`** | The troops have landed |
| 5 | `dcl` | the target castle holds the units, the source does not | Confirms |

## The exact lines (20 Travelling Knights, green to Fire Peaks, account `ventrilo`)

```
%xt%EmpireEx_21%dcl%1%{"CD":0}%
%xt%EmpireEx_21%kpi%1%{}%
%xt%EmpireEx_21%kut%1%{"SCID":16011862,"SKID":0,"TKID":3,"CID":-1,"A":[[655,20]]}%
%xt%EmpireEx_21%msk%1%{"MST":"MS5","KID":"3","TT":"1"}%
%xt%EmpireEx_21%msk%1%{"MST":"MS5","KID":"3","TT":"1"}%
```

`EmpireEx_21` is the server header of that account's world; `16011862` the green castle holding the knights
(unit `655`), `3` Fire Peaks. Send the `kut` alone and wait for its reply before the first `msk`.

## Facts this relies on

* `AID` in `dcl` is the castle id, and it is the `SCID` of `kut`.
* `kpi` is a snapshot: no `UT` for the target kingdom means nothing is travelling.
* One unit transfer is shown per target kingdom at a time.
* Every packet carrying state (`gcu`, `kpi`, `sce`) is stored as it arrives and nothing waits for another.
* An unanswered `kut` is read back with `kpi`, never repeated.

## Not established

Error statuses for `kut` and `msk`; `CID` other than `-1`; skip items other than `MS5`; a second transfer into a
kingdom still receiving one.
