# `mcm`: Recall a dispatched support march

| | |
| --- | --- |
| Direction | client to server, with a reply |
| Evidence | captured |
| Code | `empire-core/src/station.rs` (`mcm_packet`), `empire-daemon/src/direct/station.rs` |

## Purpose

Calls a support march back.

## Request

```
%xt%EmpireEx_49%mcm%1%{"MID":156018970}%
```

`MID` is the new support movement id from the successful [`cds`](cds.md) reply. It is **not** the incoming attack's `MID` from `gam`.

## Reply

```
%xt%mcm%1%0%{"A":{"M":{"MID":156018970,"PT":1044,"TT":1349,"D":1,...},"A":[[602,5]],...}}%
```

The recalled march is `A.M.MID`. At the reply, `TT - PT = 305` seconds is the estimated remaining return time. `D:1` appears on the returning movement; its general meaning needs confirmation.

## Rules and behaviour

* Store **both** ids per job: the attack's `MID` (to correlate [`mrm`](mrm.md)) and the support `MID` (for `mcm`).
* Send one `mcm` at the estimated landing of the attack, or promptly after a confirmed early retreat. If a retreat arrives while `cds` still awaits its answer, wait for the support `MID`; never guess it.
* The commander stays occupied until the return is confirmed or conservatively estimated; a successful recall does not free it at once.
* A timeout or connection loss after sending is an uncertain recall, not a reason to send a duplicate. Persist outstanding support ids across restarts. A recorded successful `mcm` reply can reconcile an uncertain state even when the player recalled by hand. A missing local XT header means nothing was injected and it can be retried.
* The proxy proof of concept cannot guarantee the server accepted a recall when the acknowledgement is missing.

## Handling in the code

Step 4 of [`sequences/support.md`](sequences/support.md).

## Captures

[`mcm_request.txt`](captures/mcm_request.txt), [`mcm_response.txt`](captures/mcm_response.txt).

## Open questions

What `D` means; refusal statuses.
