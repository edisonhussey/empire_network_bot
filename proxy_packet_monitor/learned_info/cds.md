# CDS: commit a support march

The captured client request below dispatches support. `SID=437391` matches the source castle's `SCID` in the preceding `sdi` reply, not a movement MID. `TX,TY` are destination coordinates; `LID` is the selected server lord/commander ID. `A` contains `[troop_id,count]` rows. The capture uses `WT=1`, `HBW=-1`, and zero values for `BPC`, `PTT`, and `SD`; their general allowed values need more captures.

%xt%EmpireEx_49%cds%1%{"SID":437391,"TX":468,"TY":507,"LID":0,"WT":1,"HBW":-1,"BPC":0,"PTT":0,"SD":0,"A":[[602,5]]}%
troop id 602 and 5 of them in this case, hbw = -1 means no horse nothnig 
The inbound `cds` success reply in [cds.txt](cds.txt) contains `A.M.MID`, `PT`, `TT`, `KID`, source `SA`, destination `TA`, and troop array `A.A`. Match the reply's source area ID and destination coordinates to the pending request before confirming it. A request injection or timeout alone is not confirmation. A timed-out send may have succeeded; reconcile before retrying.


The reply also contains `O[]` player details, much like `gam`. Match players by ID rather than relying on `O[0]` and `O[1]` positions.
