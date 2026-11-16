# `crin`: crafting buildings of the castle

| | |
| --- | --- |
| Direction | both: client request, server reply |
| Evidence | captured (2026-11-16, us1) |
| Code | none in rust_distributable yet. proxy_packet_monitor's **Test refine** reads it (`refineTargetFromCrin`) |

## Purpose

Sent by the client when castle view opens, together with [`dcl`](dcl.md). The reply lists each crafting building with its queue
state. It is where [`crst`](crst.md) gets the building's `OID`.

## Request and reply

```
%xt%EmpireEx_21%crin%1%{}%
%xt%crin%1%0%{"CAI":[{"CBI":[{"KID":0,"AID":16011862,"OID":393,"CQID":1,"QS":{...},"PS":{...},"S":4,"WID":2174},{"KID":0,"AID":16011862,"OID":394,"CQID":2,...,"S":12,"WID":2175}],"CE":[[377,[1,2],"BG"],[616,[1,34,23,56,12,45],"BG"],...]},{"CBI":[],"CE":[]},...]}%
```

* `CAI`: one entry per kingdom (only the first, Great Empire, had buildings here).
* `CBI`: one entry per crafting building: castle (`KID`, `AID`), building object (`OID`), queue type (`CQID`), and the same
  `QS`/`PS` queue objects as the `crst` reply. `CQID` 1 (`WID` 2174) is the refinery that crafts recipes 1 and 34.
* `CE`: recipe lists by event or building (`616` lists 1, 34, 23, 56, 12, 45). Not interpreted.

## Captures

[`crin_exchange.txt`](captures/crin_exchange.txt).

## Open questions

What `S` (4 and 12) and the `CE` group ids (377, 616) mean.
