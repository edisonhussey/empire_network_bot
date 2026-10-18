# MCM: recall a dispatched support march

Captured client request: `%xt%EmpireEx_49%mcm%1%{"MID":156018970}%`. This MID matches the new support movement MID in the successful `cds` response in `cds.txt`. It is **not** the incoming attack MID from `gam`.

Captured successful response (abbreviated; full response in [`mcm_response.txt`](mcm_response.txt)):

`%xt%mcm%1%0%{"A":{"M":{"MID":156018970,"PT":1044,"TT":1349,"D":1,...},"A":[[602,5]],...}}%`

The response identifies the recalled march by `A.M.MID`. At the response time, `TT − PT = 305` seconds is the estimated remaining return time. The LID should remain occupied until the return is confirmed or conservatively estimated; a successful recall response does not mean the commander is immediately free. `D:1` appears on the returning movement in this capture, but its general meaning still needs confirmation.

Auto Support should store both MIDs per job: the incoming attack MID for `mrm` correlation, and the support MID for `mcm`. After support dispatch, send one `mcm` at estimated attack landing, or promptly after a confirmed early retreat of the incoming attack. If retreat arrives while `cds` is awaiting acknowledgement, wait for the support MID before recalling; never guess it. Track MCM acknowledgement by the support MID. A timeout or connection loss after injection is an uncertain recall, not a reason to send a duplicate. Persist outstanding support MIDs across restarts. A recorded successful MCM reply can reconcile an uncertain state even when the player sent the recall manually. A missing local XT header means injection did not happen and can be retried when the header appears. The proxy proof of concept implements this sequence but cannot guarantee the server accepted a recall when acknowledgement is missing.

