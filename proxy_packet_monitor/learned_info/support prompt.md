# OpenAuto: deadline-triggered castle support

Implementation brief for an AI working in this folder. Investigate the live protocol and current Rust architecture before coding. This is a critical operation; report measured outcomes rather than claiming a hard network guarantee.

## Configuration and attack detection

- Configure owned **source castles** as `(KID, x, y)` tuples and one **support destination** `(x, y)` per `KID`. Scope both to the connected account. The attacked castle supplies troops; its configured destination receives them. Validate this interpretation against the captured `sdi`/`cds` packets.
- Observe inbound `gam` on the direct Rust connection. Inspect every `payload.M[]` record. Match `M.KID` and target `M.TA[1:3]` against the owned tuple set; key work by `(account_id, MID)`. Reuse the PVP parser, owned-castle list, ledger, and inbound `mrm` handling. Do not depend on UI polling or a second detector.
- Use the operator's Auto Support trigger: the target must match an owned `(KID,x,y)` tuple, and the GAM sender and receiver must have different alliance IDs. Resolve both `O[]` records by player ID (`SID` or fallback `OID` for sender; `TID` for receiver). Skip missing records, missing `AID`, or equal `AID` values. `-1` is the observed no-alliance value, so `-1` versus `-1` is skipped. This is an operational rule, not a separately verified attack-type field; neither `GA` versus `GS` nor UI arrow colour proves hostility.
- On first observation, estimate `T = received_at + max(0, TT - PT)` seconds. Preserve the original estimate for duplicate GAMs. Convert the deadline to a monotonic timer; account for whole-second packet precision and transport delay. Cancel pending work on a confirmed inbound `mrm`; never rearm a retreated MID. Skip or explicitly mark late detections.
- If an inbound `mrm` arrives after `cds` was injected, preserve the job and recall its support march after the `cds` response provides the support MID. If support was already confirmed, request recall promptly. A retreat before `cds` injection cancels pending dispatch.

## Deadline, LID, and scheduler

Let `D = T - 5 seconds`. Prepare before `D`; start the first time-critical action at `D`; aim to have the final successful `cds` acknowledgement before `T`. Measure the entire `D`-to-ack sequence and actual timing relative to `T`. Five seconds is an end-to-end target, not a per-command allowance.

- Reserve one real server **LID** well before `D`, from the existing commander ledger. Prefer one already free with the least effect on ordinary attacks; otherwise use a predicted return only with margin before `D`. Make the lease atomic and exclusive across support and attack scheduling. The `gli.C` roster does not prove availability. Revalidate the LID immediately before `cds`. Release or reconcile on confirmation, refusal, timeout, retreat, missed deadline, and reconnect; an uncertain send stays reserved until resolved.
- Add deadline awareness to the existing single-flight pipeline. Create a protected interval before and during `[D,T)`. Before starting another attack handshake, map scan, recruitment action, or stochastic wait, test its worst-case remaining duration, response timeout, protocol floor, and effect on this interval. Defer work that could occupy the connection at `D`. At `D`, support has the highest runnable priority. Do not abandon an uncertain `cra` or queue duplicate sends. The current `ActionScheduler` draws each interval only after the previous action and does not itself look ahead.
- For overlapping attacks, order by earliest `T`, then MID; account for source inventory and the single-flight transport. State a feasible capacity policy. If a LID or the window is unavailable, record a blocked/missed outcome rather than sending silently after landing.

## Support sequence and troop inventory

The current `support prompt.md` sequence was inaccurate: the capture shows `sdi` opening support details and **`cds` dispatching the support**. Verify outbound and inbound roles and success/refusal responses from real captures.

1. Establish kingdom/map context only if needed. A `gaa` can inspect/open a tile but does not by itself select the support target. Perform safe context work before `D`.
2. About two seconds before the five-second window, request `dcl` with `{"CD":1}`. Require `DCL.PID == GAM.M.TID` to confirm the target belongs to the connected account, and reject `DCL.PID == GAM.M.SID` (an own outward movement). Match `C[].KID` and `AI[].AID` to the attacked source castle's kingdom and area ID (`TA[3]` in the observed GAM). Read its `AC` inventory, excluding zero counts and IDs outside the known troop catalog. The DCL sample has only one castle; validate multi-castle replies.
3. Send `sdi` with attacked castle `SX,SY` and configured destination `TX,TY` at the start of the five-second window. Validate its response and source castle identity (`SCID`). See `sdi.txt` and `sdi.md`.
4. Cross-check **fresh troop count at the attacked source castle**. The captured `sdi` response contains `gui.I` as `[unit_id,count]` home inventory; `gui.TU` denotes troops out. Intersect it with the recent matching DCL `AC` and known troop IDs; use the smaller positive count. Do not mistake the reply's `S` formation or units out for available home troops.
5. Greedily select at most **10 distinct troop IDs** with positive home count, sorted by descending available count and then ascending ID. Allocate from the largest stock first, subject to verified support capacity and unit eligibility. The proxy proof of concept uses each selected ID's full verified home count; revise this after observing a total capacity rule. Atomically account for competing sends. Serialize `A: [[unit_id,count], ...]`, with unique IDs and no zero counts.
6. Send `cds` with the source castle ID (`SID` in the capture; verify exact meaning), configured `TX,TY`, reserved `LID`, chosen `A`, and the observed support fields (`WT:1`, `HBW:-1`, `BPC:0`, `PTT:0`, `SD:0` unless protocol evidence requires otherwise). Match its response to source, destination, and selected troops; confirm success before marking complete. The captured success response contains a movement MID, PT, TT, and A. Do not substitute `cra` or treat `sdi` as the dispatch.
7. Store the **support movement MID** from the confirmed `cds` response separately from the incoming attack MID. At `T`, or promptly after confirmed inbound `mrm` for the incoming attack, send `mcm` with `{"MID": support_mid}` on the authenticated connection. If `cds` is still awaiting acknowledgement, mark recall due and wait for its MID. On successful `mcm`, require `A.M.MID == support_mid`; `TT − PT` gives estimated remaining return time. Do not treat the LID as free at recall acknowledgement. If `mcm` times out or the connection drops after injection, mark recall uncertain and reconcile before any retry. Persist outstanding support MIDs through restart and keep recall processing active when Auto Support is toggled off. See `mcm.md`.

Use the existing authenticated connection and request correlation. Build `cds` as soon as fresh `sdi` inventory arrives. Prefetch only what remains safe to validate at send time. Avoid kingdom switching and exploratory `gaa` during `[D,T)` unless the protocol requires them.

## Bounded intervals and failures

Use the existing timing generator and diagnostics for logical action waits. At each response compute remaining deadline budget. Draw the usual interval samples, then normalize **optional** waits together to fit that budget after reserving measured network/processing time. Never compress a verified server/protocol minimum or global rate limit. If mandatory time exceeds the budget, mark the operation infeasible; no timing algorithm can guarantee a five-second completion under unbounded latency. Do not busy-wait or fire a burst of overdue actions. Record sampled versus applied intervals and constraints.

Bound request timeouts by the remaining window. A `cds` timeout is an **uncertain send**, not permission to repeat it: reconcile with acknowledgement or a matching movement first. The five-second dispatch deadline does not apply to the later `mcm` recall; schedule that against attack landing or early retreat. Do not retry an uncertain `mcm` blindly. Do not retry the same invalid payload, insufficient-troop request, or busy LID unchanged. Log both MIDs, source/destination, LID, inventory snapshot, chosen units, due/actual times, responses, recall status, and return estimate without session secrets.

## Repository pointers and acceptance checks

- Packet notes and captures: `proxy_packet_monitor/learned_info/{incoming_gam,sdi,cds,mrm,mcm}.{md,txt}` where present.
- Movement/ownership: `rust_distributable/crates/empire-core/src/pvp.rs`, `store/pvp.rs`, `crates/empire-daemon/src/direct/pvp.rs`.
- Connection and sequencing: `rust_distributable/crates/empire-daemon/src/direct/{mod,packets,automation,scheduler}.rs`.
- Commander and timing: `rust_distributable/crates/empire-core/src/store/ledger.rs`, `pacing.rs`, `timing.rs`; architecture docs `attack-scheduling.md`, `timing.md`, `pvp.md`.

Verify with labeled GAM fixtures for classification, target matching, duplicate MID and retreat; captured `sdi`/`cds`/`mcm` fixtures for source inventory, exact dispatch fields, support MID and recall response correlation; unit tests for the ten-ID greedy ordering and capacity; fake-clock tests for advance LID reservation, protected scheduling, overlapping attacks, protocol floors, timeouts, reconnection, early retreat during pending CDS, confirmed recall, and feasible/late/infeasible five-second outcomes. Run relevant Rust tests and report live latency separately from simulated timing.




