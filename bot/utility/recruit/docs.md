# Recruitment protocol mechanics observed in game traffic

This file records what the game client and server actually did. It intentionally
does not describe bot scheduling preferences, desired delays, or unobserved
behaviour.

## Evidence

The sequence below was captured for account Ventrilo on 2026-10-01 in:

- `bot/account_data/ventrilo/logs/gge_20261001_142750.log`
- `bot/account_data/ventrilo/logs/gge_20261001_142800.log`

The player began on the map, opened the Burning Sands castle, opened recruiting,
recruited crossbowmen, requested alliance help, and then opened the main castle
in the Green Kingdom.

Known identifiers in this capture:

| Meaning | Value |
| --- | ---: |
| Green Kingdom | `KID=0` |
| Burning Sands | `KID=1` |
| Main Green castle | `AID/CID=16011862` |
| Sands castle | `AID/CID=16366514` |
| Crossbowman | `WID=607` |
| Recruitment lane used | `LID=0` |

## Login castle inventory and initial view

A later relog capture, `gge_20261001_150308.log`, established the startup
mechanics. The successful login `gbd` response embedded the full owned-castle
inventory under `gcl.C`. Each kingdom block carried `KID`; each `AI.AI` row
carried area type, coordinates, castle ID, and name.

The observed owned inventory was:

| KID | Type | Castle ID | Coordinates | Name |
| ---: | ---: | ---: | ---: | --- |
| 0 | 1 | 16011862 | 509,405 | `._.` |
| 0 | 4 | 16632819 | 505,407 | `vrvr` |
| 0 | 4 | 16673031 | 509,407 | `Outpost Ventril` |
| 1 | 12 | 16366514 | 593,613 | `Castle Ventrilo` |
| 2 | 12 | 16366513 | 654,696 | `Nui` |
| 3 | 12 | 16681051 | 688,582 | `fire` |

In this inventory, type `1` was the Green main castle, type `4` represented the
Green outposts, and type `12` represented castles in the outer kingdoms.

At `15:03:10.581`, a successful server `jaa` reported `KID=0` with active area
row `A=[1,509,405,16011862,...]`. This confirms that the session began inside
the Green main castle rather than in map view.

At `15:03:15`, opening the Green map emitted two `gbl` requests followed by
multiple `gaa` requests whose payloads had `KID=0` and map chunk coordinates.
The `gaa` traffic therefore provides a positive observation that general map
view is active in the named kingdom. `gbl` alone does not carry a kingdom ID.

## Castle and kingdom navigation

The client selects a castle with `jca`:

```text
jca {"CID":16366514,"KID":1}
```

The successful response is `jaa` with status `0`. Its payload contains the
selected kingdom and a large snapshot of the selected castle, including castle
layout, owner data, resources, units, and recruitment-lane state.

The same command moves directly between kingdoms. After recruiting in Sands,
the client opened the main Green castle with:

```text
jca {"CID":16011862,"KID":0}
```

That also received `jaa` status `0`, with `KID=0` and the Green castle state.
No intermediate map command was emitted between these two castles.

## Opening recruitment data

After `jaa` confirmed the Sands castle, the client sent four requests close
together:

```text
dcl {"CD":0}
gpa {}
spl {"LID":0}
gui {}
```

Their observed responses were:

- `dcl`: a kingdom-grouped list of the account's castles and their state.
- `gpa`: current production, capacity, protection, and resource-related values
  for the selected castle.
- `spl`: recruitment state for the requested lane.
- `gui`: unit inventory for the selected castle.

Immediately before recruiting, lane `0` had an empty `PS`, no queued unit
payload in `QS`, and `TCT=0`:

```json
{
  "PS": {},
  "QS": [
    {"SI": {"RUT": -1, "VIP": 0}},
    {"SI": {"RUT": -1, "VIP": 0}},
    {"SI": {"RUT": 2298401, "VIP": 1}},
    {"SI": {"RUT": 2298401, "VIP": 1}},
    {"SI": {"RUT": 245133, "VIP": 0}}
  ],
  "RM": 0,
  "TCT": 0,
  "LID": 0
}
```

The `SI` entries exist even when no troop payload is occupying the slot. Their
fields were not manipulated in this capture, so their complete semantics are
not established here.

## Creating a recruitment order

The client recruited 190 crossbowmen with one `bup` request:

```text
bup {"LID":0,"WID":607,"AMT":190,"PO":-1,"PWR":0,"SK":73,"SID":1,"AID":16366514}
```

Fields whose meaning is directly supported by the surrounding state:

| Field | Observed role |
| --- | --- |
| `LID` | Recruitment lane; the response returned the same lane. |
| `WID` | Unit type; `607` is crossbowman. |
| `AMT` | Requested quantity; `190` was split into active and queued quantities totalling 190. |
| `SID` | Kingdom containing the castle; `1` is Sands. |
| `AID` | Castle receiving the recruitment order. |

`PO=-1`, `PWR=0`, and `SK=73` are exact observed values. This capture does not
establish their general meanings or whether they can be reused for every troop,
castle, or account.

The successful `bup` response had status `0` and included updated `spl`, `grc`,
and `gcu` objects. Its recruitment state was:

```json
{
  "PS": {
    "WID": 607,
    "TUA": 5,
    "CBS": 0,
    "RAH": false,
    "PID": 570104746,
    "RCT": 75,
    "ICT": 75,
    "SPID": 313036191
  },
  "QS": [
    {
      "P": {
        "WID": 607,
        "TUA": 185,
        "CBS": 0,
        "RAH": false,
        "PID": 313036191
      },
      "SI": {"RUT": -1, "VIP": 0}
    }
  ],
  "RM": 0,
  "TCT": 2850,
  "LID": 0
}
```

Observed mechanics:

- The order became an active production quantity of `5` in `PS` and a queued
  quantity of `185` in the first `QS.P`: `5 + 185 = 190`.
- `WID=607` appears in both the active and queued production state.
- `RCT=75` and `ICT=75` accompanied the active production batch.
- `TCT=2850` represented the total remaining recruitment time at the moment of
  the response. It is exactly `190 × 15` seconds in this observation.
- `RAH=false` before alliance help was requested.
- The response's `grc.F` changed from `631654.10` to `631625.06`, a decrease of
  `29.04`, at recruitment creation. No other resource interpretation is asserted
  from this single sample.

## Alliance recruitment help

The client requested help for recruitment lane `0` with:

```text
ahr {"ID":0,"T":6}
```

The server then broadcast an `ahh` object that identifies the request as
recruitment help:

```json
{
  "TID": 6,
  "P": 0,
  "RT": 2848,
  "OP": {"AID": 16366514, "SID": 1, "RLID": 0}
}
```

The `OP` object ties the help request to Sands castle `16366514` and recruitment
lane `0`. Subsequent `ahf` messages named alliance members who helped, while
later `ahh` broadcasts for the same request advanced `P` through `1`, `2`, and
`3`.

After the third observed help, the server sent updated `spl` state:

```text
PS.TUA: 5 -> 6
QS[0].P.TUA: 185 -> 222
RAH: false -> true
TCT: 2850 -> 2846
RCT: 75 -> 71
```

This changed the total produced quantity from `190` to `228`, an increase of
`38` or 20%. At the same time, `TCT` and `RCT` had each fallen by four seconds,
matching elapsed wall time rather than being extended by the extra units. In
this observation, alliance help therefore increased output without extending
the remaining recruitment time.

An `ahd` for the recruitment help's alliance-help list ID followed the completed
help sequence. Other `ahh`, `ahf`, and `ahd` messages in the same connection
belonged to unrelated alliance activity and cannot be associated with this
order unless their IDs and `OP` data match.

## Leaving recruitment

No distinct close-recruitment packet appeared after help. The next client
mutation was the `jca` selecting the main Green castle. Therefore, the capture
only proves that castle selection can supersede the recruitment screen; it does
not prove that a separate wire-level close operation exists.

The castle switch did not emit a recruitment-cancellation command. This capture
did not query the Sands lane again afterward, so continued queue state after the
switch is not independently confirmed here.

## Client screen and server context can diverge

A live test on 2026-10-01 established that injecting map reads is not equivalent
to the player opening the map. The client remained visibly on the castle screen
after injected `gbl`, `upt`, and `gaa` requests, but subsequent castle-scoped
requests showed that the server had entered a different context.

The sequence and responses were:

- An injected Green-main `bup` for castle `16011862` succeeded with status `0`.
- Injected Green `gaa` map chunks all succeeded with status `0`, while the
  visible client remained in castle mode.
- Later client `spl` and `gui` requests returned status `53`.
- Four further `bup` requests still succeeded because each request explicitly
  included `SID=0` and `AID=16011862`. Their returned `TCT` values accumulated
  from `5409` through `8258`, `11107`, and `13955` seconds.
- `ahr {"ID":0,"T":6}` returned status `53`; unlike `bup`, this command carries
  no castle ID and operates on the current recruitment lane.
- The player opened the Green map and then selected main castle `16011862`.
  The resulting `jca` received a successful `jaa`.
- After that `jaa`, `spl` and `gui` again returned status `0`, and the same
  `ahr` request successfully created an `ahh` recruitment-help entry for
  `AID=16011862`, `SID=0`, `RLID=0`.

This demonstrates two distinct forms of state: the screen displayed by the
client and the request context held by the server. An injected `gaa` can change
the latter without changing the former. A successful `jaa` re-establishes an
owned-castle context for commands such as `spl`, `gui`, and `ahr`.

An injected `jca`, by contrast, received a normal `jaa` castle snapshot and the
client subsequently displayed the selected castle. Returning from that castle
to general map view did not expose a corresponding network command in these
captures: the positive network observation for the transition was the first
genuine client `gaa` carrying the destination `KID`.

The manual map click captured at `20:19:49` produced `gbl {}` followed by six
`gaa` requests for adjacent 13-by-13 Sands chunks. No client request preceded
`gbl`. This confirms that the visible castle-to-map transition happens locally
before network traffic; `gbl`/`gaa` are consequences of the transition rather
than a command that causes it.

## Later controlled multi-slot observations

Later live trials on 2026-10-01 repeated the same castle bootstrap and issued
five `bup` requests for 180 crossbowmen each, once in the Fire castle and once
in the Sands castle. Each request received a successful status-`0` `bup`
response. The returned `TCT` grew cumulatively as slots were appended; it was
not a per-slot duration. A subsequent `ahr` produced recruitment-help state,
and the final server lane state was persisted for the matching castle.

These trials establish that multiple orders can be appended to lane `0` when
each `bup` is allowed to complete before the next request. They do not establish
a universal safe delay for every packet or every account.

## Traffic that is not part of creating this order

- `pin` is connection heartbeat traffic.
- `gbl` and repeated `gaa` packets are map/list reads and associated state
  refreshes. They preceded navigation but did not create the recruitment order.
- `gcu`, `sce`, and `grc` arrived as server-side update/response traffic around
  the successful `bup`; they were not separate client instructions in the
  recruitment action.
- Alliance `ahh`, `ahf`, and `ahd` traffic is global to the alliance connection.
  Request/list IDs and `OP` must be matched before treating it as state for this
  castle's recruitment.

## Not established by these captures

- The behaviour of recruitment lanes other than `LID=0` was not exercised.
- Status `53` was observed for `spl`, `gui`, and `ahr` while the server was in
  map context; other recruitment failure statuses remain unobserved.
- Cancellation was not performed; no cancellation command or refund behaviour
  is known.
- `SK=73`, `PO=-1`, and `PWR=0` are only verified for this exact successful
  request.
- No conclusion can be drawn about other unit types, kingdoms, castles, or
  accounts without additional captures.
