# State funnels: how these packets should be handled

From the note on `kut`/`msk`: after a transfer, expect a fresh `kpi`, but **do not write code that waits
for it**. Each packet type has its own funnel that is always listening, applies what it carries and
returns. Nothing blocks on another funnel, so there is no ordering to get wrong.

| Packet | Funnel | Applies | How |
| --- | --- | --- | --- |
| `gcu` | currency | coins `C1`, rubies `C2` | set both to the values given |
| `sce` | skip stock | `[key, count]` rows | set that item's count |
| `kpi` | kingdoms | `UL` unlock rows, `UT`/`RT` transfers | **replace** the whole transfer list (a `kpi` without `UT` means none) |
| `kut` / `msk` reply | action reply | status, then the `kpi` inside | hand the nested `kpi` to the kingdom funnel unchanged; read the status for success or error |
| `dcl` | castle inventory | `C[].AI[]` castles and their `AC` | replace that castle's stock |
| `rue` | castle inventory | one unit in one castle | set `NUA` for `WID` in castle `AID` |
| `acm` | chat | message text | append |

## Rules that keep it race-free

1. **Absolute values, not deltas.** Every funnel sets state from what the packet says (a count, a seconds
   remaining), never "add one" or "subtract the cost". Arrival order then hardly matters, and a repeated
   packet does nothing new.
2. **Key by id.** Castle by `AID`, kingdom by `KID`, skip item by its `MS` key, transfer by target `KID`.
   Two kingdoms' updates never touch the same entry.
3. **A send is decided from the current snapshot**, not from a reply being awaited. Before `kut`: the unit
   count in the source castle, the coins, the target kingdom unlocked and no `UT` for it. Before `msk`:
   a `UT` entry exists for that `KID`, with `RS > 0`.
4. **Confirmation is a state change.** A transfer is confirmed when `UT` for that `KID` appears; landed
   when it disappears. A reply status other than `0` is an error. A missing reply is not proof of
   failure: look at the next `kpi` before sending again, so a duplicate is never sent.
5. **One transfer in flight per target kingdom** (`UT` has one entry per `KID`), so the send-gate is a
   per-kingdom flag that clears when the `kpi` shows no `UT` for it.
6. **Remaining time is relative**: `RS` counts from the packet's arrival, so store `arrival + RS` as an
   absolute end time (the client does the same) and read the remaining time from the clock.

This is the reason `dcl` is read before `kut` (to seed the inventory funnel) but is not a precondition
the code sits and waits on: if the stock is already known it is not asked again.
