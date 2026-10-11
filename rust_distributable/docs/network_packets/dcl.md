# DCL: castle overview and inventory before support

The captured client request is `%xt%EmpireEx_49%dcl%1%{"CD":1}%`. The successful server reply in [dcl.txt](dcl.txt) has `PID` and `C[]`. Each `C[]` entry has a `KID` and `AI[]`; each area has `AID` (castle area ID) and `AC: [[item_id,count], ...]`. In the capture, kingdom 0, area 437391 has `AC=[[602,9],[614,5],[660,3],[637,0]]`. The zero-count row is not available inventory.

`DCL` appears callable from map view and can refresh the inventory shortly before support. The observed `AC` list does **not** label whether each ID is a troop or a tool. Match `C[].KID` and `AI[].AID` to the attacked castle; filter against known troop IDs and corroborate with the `sdi` reply's `gui.I` before constructing `cds.A`. Do not apply one castle's `AC` to another. The sample includes only one castle, so verify multi-castle reply layout with another capture.

The proxy proof of concept asks for `dcl` about seven seconds before estimated landing, which is two seconds before the intended five-second support window. In the sample, ID 602 is a known troop; IDs 614 and 660 are excluded by the current troop catalog. It sorts positive, verified home troop quantities descending and takes up to ten distinct IDs. The observed packets do not prove a universal unlimited-per-type rule or a total support capacity; a server refusal must be recorded, not retried unchanged. Coin sufficiency is assumed only for this proof of concept.

