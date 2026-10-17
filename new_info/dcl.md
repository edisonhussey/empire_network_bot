# `dcl`: castles and the units in each

The overview of every castle the account owns, with the troops and tools in each. This is the "sync
before you build a packet" step for anything that moves units (`kut`, `cds`).

```
%xt%EmpireEx_21%dcl%1%{"CD":0}%
```

`CD` is `0` here and `1` in the older capture in `proxy_packet_monitor/learned_info/dcl.md`; what it
selects is not known (both returned the full list). Reply, abbreviated (full text in
[`dcl_example.md`](dcl_example.md)):

```
{"PID":16862926,"C":[{"KID":0,"AI":[{"AID":16011862,"W":804000.0,...,"AC":[[655,20],[277,9890],...]}, ...]}, ...]}
```

| Field | Meaning |
| --- | --- |
| `PID` | the **player** id |
| `C[].KID` | kingdom |
| `C[].AI[].AID` | the **castle** id (not the account). It is what `kut` calls `SCID`; the `rue` and `dcl` for the same castle share it |
| `AI[].AC` | `[[item_id, count], ...]` for that castle only: troops **and** tools mixed, not labelled |
| `W S F C O G A I ...` | the castle's stock of wood, stone, food, coins and the other goods |

In the account's capture: green (`KID` 0) has three castles (16011862 with the big army, 16632819,
16673031), kingdom 1 has 16366514, kingdom 2 has 16366513, kingdom 3 (Fire Peaks) has 16681051, kingdom 4
none. Unit ids are in the items file `units` list (277 is "Elite Tinoswolves Variant", 655 is the
"Travelling Knights"); a tool is not a unit, so filter `AC` against that list. The green castle with 655
is 16011862, which holds 20 of them. See also `proxy_packet_monitor/learned_info/dcl.md`.

`rue` (`{"AID":16011862,"SID":0,"WID":217,"NUA":31039,"RUA":5}`) updates one castle's count for one unit
as units are recruited: `AID` castle, `WID` unit id, `NUA` the new total (matches `[217,31039]` in the
`dcl`). Keep stock by applying such updates as they come rather than re-asking `dcl` each time.
