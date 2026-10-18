# Incoming movements from GAM packets

This note records what is known about incoming movements. It is based on the [supplied GAM fixture](gam_sample.txt), a second packet supplied for an alliance target ([gam.txt](gam.txt)), the user's comparison of timing values across packets from the same movement, local September 2026 GAM captures, and the owned-castle attack fixture [gam_156095506.txt](gam_156095506.txt). The parser is `crates/empire-core/src/pvp.rs`; the PVP record is described in [`../architecture/pvp.md`](../architecture/pvp.md). Treat these as observed packet shapes, not a complete GAM schema.

## Confidence and scope

| Status | Meaning |
| --- | --- |
| **Observed** | A field and value occur in the supplied packet. |
| **Established for this POC** | The timing or matching rule was provided and is exercised by tests. |
| **Unknown** | The packet does not establish the field's meaning or location. Keep the original data for later inspection. |

The detector processes incoming server `gam` movements and inbound confirmed `mrm` retreats. A matching movement is an **incoming candidate**. Auto Support applies the owned-castle and differing-alliance rule below. A calculated ETA is not confirmation that the movement arrived.

## Wire and decoded structure

The sample frame begins:

```text
%xt%gam%1%0%{"M":[...],"O":[...]}%
```

The current XT parser returns `command="gam"`, `request_id="1"`, `status="0"`, and a JSON object in `payload`. The useful hierarchy is:

```text
XT frame
└─ payload
   ├─ M[]                     movement records; inspect every element
   │  └─ M[i]
   │     ├─ M                 movement metadata (MID, PT, TT, KID, SA, TA, ...)
   │     ├─ UM                observed; full meaning not established here
   │     ├─ GA or GS          observed in different samples; meaning not established here
   │     ├─ AST               observed; meaning not established here
   │     └─ ATT               observed; meaning not established here
   └─ O[]                     related player records, matched by OID
```

The repeated `M` is deliberate: `payload.M` is an array, while `payload.M[i].M` is the nested movement object. Code must loop through **all** `payload.M[]` elements. A malformed element can be skipped without discarding other movements in the frame.

### Movement metadata

All paths below are relative to `payload.M[i]`.

| Path | Sample | Interpretation used now |
| --- | ---: | --- |
| `M.MID` | `155072784` | Unique movement ID; ledger key. |
| `M.PT` | `464` | Seconds already travelled at observation. |
| `M.TT` | `557` | Total travel duration in seconds. |
| `M.KID` | `0` | Kingdom ID used for target matching. |
| `M.SID` | `438615` | Source player ID used to find a player in `payload.O[]`. |
| `M.OID` | `438615` | Observed source ID in this fixture; used as fallback if `SID` is absent. Its general meaning needs more packets. |
| `M.TID` | `438627` | Target player ID used to find a player in `payload.O[]`. |
| `M.SA` | `[1,647,759,...]` | Source location array. X is index `1`, Y is index `2`. |
| `M.TA` | `[1,677,688,...]` | Target location array. X is index `1`, Y is index `2`. |

**Direction matters:** `SA[1:3]` gives the sender `(647, 759)`; `TA[1:3]` gives the receiver `(677, 688)`. The leading array element and other positions in `SA` and `TA` have not been assigned meanings in this POC. Do not infer them from this single example.

### Related players

`payload.O[]` contains player objects. Use IDs instead of array positions: find the sender where `O[j].OID == M[i].M.SID` (falling back to `M[i].M.OID` if `SID` is missing), and find the receiver where `O[j].OID == M[i].M.TID`. In the fixture `O[0]` happens to be the sender and `O[1]` the receiver; that ordering is not a protocol guarantee.

| Player field | Meaning used now | Sender in fixture | Receiver in fixture |
| --- | --- | ---: | ---: |
| `OID` | Player ID | `438615` | `438627` |
| `N` | Player name | Great Emperor 7 | Great Emperor 8 |
| `L` | Player level | `7` | `6` |
| `LL` | Legendary level | `0` | `0` |
| `MP` | Might points | `81862` | `353` |

If a matching `O[]` record or an optional field is absent, keep the movement and show the missing value as **Unknown**. The sender and receiver coordinates still come from movement `SA` and `TA`, not from the players' array positions.

## ETA calculation

The timing relationship established for this POC is:

```text
remaining_at_observation_seconds = TT - PT
arrival_at_utc = received_at_utc + (TT - PT) seconds
live_remaining_seconds = max(0, arrival_at_utc - now_utc)
```

For the fixture, `557 - 464 = 93` seconds remain when the packet is received. If a test uses `received_at = 1000`, it stores `arrival_at = 1093`. Store the numeric UTC epoch timestamp at **first observation**; convert it to local time only for display. A later GAM containing the same MID does not revise that timestamp. Network delay and whole-second PT/TT precision can make the estimate slightly inaccurate.

The web page updates its countdown from the stored arrival timestamp about once per second, without needing another GAM packet. Once `now >= arrival_at`, the entry moves out of the active count into **Predicted arrival passed**. That state is based only on the ETA.

## Current incoming-candidate rule

The PVP record and the station rule both start from the target `(KID, x, y)`: a movement is aimed at an owned castle when all three match one of the account's castle tuples found at initialisation:

```text
M[i].M.KID   == own_kid
M[i].M.TA[1] == own_x
M[i].M.TA[2] == own_y
```

For the fixture, `(own_kid, own_x, own_y) = (0, 677, 688)` matches. `(0, 647, 759)` does **not** match: those are the sender coordinates. This rule does not establish that the movement is hostile. Keep any later hostile/support distinction in the isolated `is_incoming()` predicate after it is verified from real packets.

The Rust record is persistent and keyed by `(account_id, MID)`. **First observation wins** for the ETA and player details. A later GAM for the same MID may fill an unknown composition when the first packet had only `GS`. Multiple distinct MIDs are tracked independently. See [`../architecture/pvp.md`](../architecture/pvp.md).

An inbound successful [`mrm` frame](mrm.md) containing its `MID` moves a known entry to **Retreated**. The user reports that retreat is irreversible and the MID is terminal. A request alone is not a confirmed retreat.

## GA troop counts; tools and waves still unknown

The original supplied `payload.M[0]` has only `GS`, so troop quantity remains Unknown in that version. A later GAM can carry `GA` with arrays `L`, `M`, `R`, `RW`, each containing `[unit_id,count]` pairs. Sum all four groups for the march's visible troop count. `UM.L.EQ` is commander equipment and must not be counted as troops or tools; `UM.PWD` and `UM.TWD` are not verified totals. Tool and wave counts remain Unknown.

For the [latest owned-castle attack fixture](gam_156095506.txt), `TA[1:3]=[543,520]` is the receiver and `SA[1:3]=[546,520]` the sender. `TID=596904` resolves to receiver alliance `3026`, while `SID=432945` resolves to sender alliance `3652`; their positions in `O[]` are reversed from the earlier fixture. `TT-PT=185-4=181` seconds remain at observation. `GA.M=[[10,22]]` means 22 of unit 10 (Veteran Deathly Horror); `GA.RW=[[655,67]]` means 67 of unit 655 (Traveling Knight). `L` and `R` are empty, so **22 + 67 = 89 visible troops**. The operator describes the RW amount as courtyard attack troops; the packet key alone does not establish its exact battle role, so the UI labels it RW / reserve-courtyard. It is included in the total. `GS` alone is still Unknown, and a later GA fills that value without moving the original ETA.

That MID was an attack on an owned castle that was retreated before the support window opened; the proof-of-concept proxy recorded it as **Retreated** and never reached the DCL/SDI/CDS dispatch window.

## Comparing the alliance-target and original samples

The later packet was described as an attack on an alliance member. Its target is **not the same target** as the original fixture: `TID=432945` at `(546, 520)` versus `TID=438627` at `(677, 688)`. The packets also have different `MID`s. This comparison therefore cannot isolate an own-target versus alliance-target difference.

| Field | Alliance-target sample | Original fixture |
| --- | --- | --- |
| `M.MID` | `155307796` | `155072784` |
| `M.SID`, `M.SA[1:3]` | `596904`, `(543, 520)` | `438615`, `(647, 759)` |
| `M.TID`, `M.TA[1:3]` | `432945`, `(546, 520)` | `438627`, `(677, 688)` |
| `M.PT`, `M.TT` | `4`, `57` (53 seconds remaining) | `464`, `557` (93 seconds remaining) |
| `M.HBW` | `-1` | `1003` |
| `M.KID`, `AST`, `ATT` | `0`, `[]`, `0` | `0`, `[]`, `0` |
| Movement detail | `GA={"L":[],"M":[[602,9]],"R":[],"RW":[]}`; no `GS` | `GS=3`; no `GA` |
| `UM.L.W`, `UM.L.D`, `UM.L.SPR` | `1`, `0`, `1` | `7`, `3`, `2` |
| `UM.L.AE` | `[[111,[1.0],"AB"]]` | `[]` |
| Sender and target alliance IDs (`O[].AID`) | `3026`, `3652` | `-1`, `-1` |

Both packets use the same XT command and the same outer `M[]`/`O[]` structure. The alliance-target packet includes `AN`, `AR`, and `aee` in both player objects; these accompany nonnegative alliance IDs. The sender and target in that packet have **different** alliance IDs. It does not include the observing account's identity or alliance ID, so the target's `AID` alone cannot establish that it is the observer's own castle or alliance member. Compare it to account/session state for that classification.

`GA` is structured by `L`, `M`, `R`, and `RW`, and its middle entry is a two-number pair. The keys suggest lanes, but neither the meaning of `[602,9]` nor the meaning of `GS=3` is verified. In the local captures, the **same MID** can appear first with `GS` and later with `GA` while `SID`, `TID`, `SA`, `TA`, and `TT` stay the same. Examples: MID `82781747` has `GS=292` at `PT=1008`, then `GA` with `L:[[215,88]], M:[[215,128]], R:[[215,88]]` at `PT=1049`; MID `82787687` has `GS=40` at `PT=518`, then `GA` at `PT=538`. The visible GA pair values do not simply sum to the earlier GS values. **Do not use GA versus GS as an own/alliance or hostile/support discriminator, or assume a composition total from either field.**

To isolate perspective, capture the same `MID` and `TID` from the owning account and an allied account, with each account's player/alliance ID recorded separately. To classify movement type, obtain labeled attack and support examples and compare their movement fields independently of viewing account.

### Auto Support operational rule

For the proxy Auto Support proof of concept, the operator supplies owned castle `(KID,x,y)` tuples. A movement is eligible only when `M.KID` and `M.TA[1:3]` match one of those tuples **and** the sender and receiver have different `AID` values. Resolve sender `O[]` by `M.SID` (fallback `M.OID`) and receiver `O[]` by `M.TID`; never rely on array positions. Both player records and both numeric `AID` fields must exist. Equal or missing IDs are skipped. `AID=-1` is the observed no-alliance value; two unaffiliated players therefore compare equal and are skipped. Before dispatch, the DCL reply must also confirm `PID == M.TID` and contain the matched castle area ID. This is the user's chosen operational definition of an attack on an owned castle, not a separately proven GAM movement-type field.

## Capture path and current limits

The packets were first captured with a mitmproxy addon watching server-to-client frames on `ep-live-*-game.goodgamestudios.com`. The Rust daemon reads the same frames directly from its own game connection (`crates/empire-daemon/src/direct/pvp.rs`), so no proxy is involved.

Known limits to revisit with additional captured GAM packets:

1. Which movement field definitively separates hostile attacks from support or other movements?
2. The `GA` troop groups are visible, but which paths reveal tool and wave counts, and what precisely does `RW` represent in battle?
3. Do all server GAM frames use the observed `%xt%gam%...%` form? The generic XT parser supports a server-header variant, while the addon currently queues only the observed prefix.
4. Are `SID`, `OID`, and `TID` consistent across multiple players, kingdoms, and movement types?
5. What do the remaining `SA`/`TA` array positions and `UM`, `GS`, `AST`, `ATT` fields mean?

The Rust fixture tests (`crates/empire-core/src/pvp.rs`, `station.rs`) cover the sample values, ID-based player lookup with reordered `O[]`, duplicate MIDs, malformed elements and missing optional players.
