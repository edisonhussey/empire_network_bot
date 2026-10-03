# Network requests

Purpose: reason about any packet without loading the 4k-line Python listener back
into context. Every fact below is marked with how it was established.

- **[capture]** — read directly from a live capture log under
  `bot/account_data/ventrilo/logs/`. Strongest evidence: it is what the real
  client actually did.
- **[code]** — read directly from `crates/empire-core/src/session.rs`, which is the
  Rust implementation that actually runs. High confidence.
- **[python]** — transcribed from `bot/`. Behaviourally verified by the bot, but not
  re-verified in Rust.
- **[unknown]** — not established. Do not guess; capture it (see the end).

When a `[capture]` fact contradicts `[code]`, the capture wins and the Rust is a bug.

Update this file whenever a request is added, removed, or its payload changes. A
stale entry here is worse than no entry.

---

## 1. Frame format

`[code]` **Client → server** frames carry the server header:

```
%xt%EmpireEx_21%<command>%<request_id>%<json payload>%
```

`[code]` **Server → client** frames **omit the header** and insert a status:

```
%xt%<command>%<request_id>%<status>%<json payload>%
```

This asymmetry is real and was confirmed by capture: a server frame reads
`%xt%irc%1%0%{"G":[["C1",10]]}%` — four fields after `xt`, with the status
where the client puts the header. Parsers must therefore branch on direction, or
detect the header by shape rather than by position.

- `SERVER_HEADER` = `EmpireEx_21` for the US1 live server.
- `command` = three lowercase letters, e.g. `lli`, `gbd`, `adi`, `cra`, `cat`.
  Note the case is not always lower — `FTF` appears in real traffic.
- `request_id` = client-chosen counter (`1` for everything we send today).
- `status` is server-only. `0` means success; non-zero is an error code (§6).
- The payload is JSON, but only the **outer frame is `%`-delimited** — JSON braces
  are not escaped, so parse by splitting on `%` with a field limit, not by
  splitting on braces.

Transport: `wss://ep-live-us1-game.goodgamestudios.com/` with the header
`Origin: https://empire.goodgamestudios.com`.

There is also a `<body action='...'>` **system frame** used during login only
(`apiOK`, `joinOK`). It is not `%xt%`-framed; treat it as an opaque trigger.

---

## 2. Login and initialisation

`[code]` Each line is: **trigger → what we send**. The client sends nothing until
the server prompts it.

| # | Trigger from server | We send | Notes |
|---|---|---|---|
| 1 | *(connection opens)* | `verChk` | Version check, sent immediately on connect. |
| 2 | `<body action='apiOK'>` | `login` | Cross-domain policy accepted. |
| 3 | `%xt%rlu%...%Lobby%` | `<body action='autoJoin' r='-1'>` | Auto-join the lobby. |
| 4 | `<body action='joinOK'>` | **2 packets** | Lobby joined. |
| 5 | `%xt%vck%1%0%<version>%<build>%` | `lli` | Version accepted → send login credentials. |
| 6 | `%xt%lli%1%0%` | *(nothing)* | `status=0` → authenticated. Any other status is a login failure. |
| 7 | `%xt%gbd%1%0%{...}%` | **9 packets**, in order: `gbl`, `jca`, `alb`, `sli`, `gie`, `asc`, `sie`, `ffi`, `kli` | Account bootstrap. The `gbd` payload is where owned castles and commanders are learned. |
| 8 | `%xt%jaa%1%0%{...}%` | `gbl` **then 6 × `gaa`** | Entering the map. The 6 `gaa` calls are the 3×2 map viewport. |
| 9 | `%xt%gaa%1%0%{...}%` for the target kingdom | *(nothing)* | Map loaded → initialisation complete. |

`[code]` The viewport is **derived, not configured**: `gbd` carries the main
castle's coordinates, and the 6 tile requests start at
`(castle_x − 21, castle_y − 15)` stepping 13 tiles
(`MapViewport::sands_default()` → 572,598 for a castle at 593,613).

`[code]` `lli` payload fields (`session.rs::login_packet`):

| Field | Meaning |
|---|---|
| `NOM` | Player name. **This is the account identity** — never the portal id. |
| `AID` | Portal account id. **Shared by every account under one login** — cannot identify an account. |
| `PW` | Password. Mutually exclusive with `LT`. |
| `LT` | Login token (alternative to `PW`). |
| `RCT` | Registration token, optional. |
| `KID` | Empty string on login. |
| `CONM`, `RTM` | Connection / round-trip times. |
| `SID`, `PL`, `PLFID`, `LANG`, `REF`, `DID` | Client metadata. |

---

## 3. Map scan — finding targets (`gaa`)

`[python]` `gaa` is "get area/action": it returns the visible grid squares.

Send: `%xt%EmpireEx_21%gaa%1%{"KID":<kingdom>,"AX1":x,"AY1":y,"AX2":x+12,"AY2":y+12}%`

- `[capture]` A scan tile is **13×13** — `AX2 = AX1 + 12`, `AY2 = AY1 + 12`. Confirmed
  by capture, and the tile origins step exactly 13 (§5).
- `[capture]` **`KID` is the kingdom selector** — this is the only place the kingdom
  is chosen. See §5.
- `[capture]` The standard viewport is **3 columns × 2 rows = 6 `gaa` calls**.
- A radius sweep walks tile origins outward from a centre point.
- Response: `gaa` with `AI` rows, one per visible object.
- `[code]` RBC rows are filtered by area type; owned castles use area types `1`,
  `4` and `12`; target rows carry a level.
- `[python]` Target level comes from the `AI` row; the level-code map is
  kingdom-specific (Storm used `{7:60, 8:70, ...}`).

Pacing: `[python]` scanning waits a bounded Gaussian between tiles
(`gaa_waiting_time` ≈ 12 s ± 3.5 s, floor 8 s). Do not send tiles back to back.
`[capture]` The capture shows all 6 viewport tiles sent within ~12 ms, so a *single
viewport batch* is one request as far as pacing is concerned — the wait applies
between batches, not between the 6 tiles.

---

## 4. Attack cycle — `adi` → `cra` → `cat`

`[python]` This is the Sands/Storm path. Berimond uses `aci` instead of `adi`.

1. **`adi`** — "attack detail / inspect".
   Send: `{SX, SY, TX, TY, KID}` (source castle, target, kingdom).
   Response: the target's `AI` row, the `gui.I` **inventory** (troops/tools
   available), and `gli.C` **commanders**.
2. *(client-side)* Validate the target's level matches the task, then pick a
   commander LID from the intersection of the target's and the task's LIDs.
3. **`cra`** — "create attack".
   Send: the full attack envelope. Response: `AAM.M = {MID, TT, TA, KID}`
   (`MID` = march id, `TT` = travel time).
4. **`cat`** — server-initiated, later, when the march resolves.
   Payload: `A.M = {SA, KID, MID, TT}`, `A.S` = result flag, `A.G` = loot
   (`[["C1",coins],["C2",rubies]]`), `A.UM.L.ID` = the lord that returned.

`[capture]` **The `cra` acknowledgement has two shapes.** Both were seen live, and a
parser that assumes `TA` is present will drop the march id on the other:

```jsonc
{"AAM":{"M":{"MID":101091153,"TT":105}}}                        // no TA
{"AAM":{"M":{"MID":101091268,"TT":127,"TA":[2,589,610]}}}      // TA present
```

`[capture]` **The `cat` march id is not the `cra` march id.** A live cycle returned
`MID=101093730` for a march acknowledged as `MID=101092279`. Do **not** join a
return to its march by id — see the attribution rule below.

`[python]` Attack envelope (`bot/bot.py::build_attack_payload`):

```
{SX, SY, TX, TY, KID, LID, WT:0, HBW, BPC:0, ATT:0, AV:0, LP:0, FC:0,
 PTT, SD:0, ICA:0, CD:99, A, BKS:[], AST:[-1,-1,-1], RW:[[-1,0]]×8, ASCT:0}
```

`A` is the wave payload (`empire_game::Attack::to_payload()`).

### Attributing a return to the march that caused it

`[capture]` + `[python]` A `cat` cannot be attributed by march id, because the march
id changes. Two facts make it work instead:

1. **The areas swap on the return leg.** `bot/rbc_proxy_listener.py::movement_area`
   reads `A.M.SA` for the result and checks the area type is `2` (`AREA_BARRON`, an
   RBC). On the outbound `cra` the RBC is the *target* (`TA`); on the return it is
   the *source* (`SA`). Reading `TA` from a `cat` yields the castle, not the RBC.
2. **Match on position plus lord.** `bot/bot.py::mark_rbc_result` selects the newest
   `status='sent'` row for that `(account, target, lord_id)`.

`empire-core` implements the same rule as
`Store::finish_march_by_target(account, kingdom, x, y, lord, …)`. Observed result:

```
LOOT march_id=Some(101093730) lord=Some(16) at=1:585:609 coins=39612 rubies=0 \
     flag=Some(0) attribution=1
```

`attribution=1` means exactly one ledger row was matched. A run that reports
`attribution=0` has a real bug: the loot happened but was not recorded.

### The usable commander pool

`[capture]` Not every lord in the roster can attack. `USABLE_COMMANDER_LIDS`
(`empire-core/src/hunt.rs`, ported from `bot/bot.py`) holds 35 ids:

```
0, 2, 3, 6, 7, 8, 9, 10, 11, 16, 17, 18, 20, 21, 22, 23, 24,
25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42
```

**LIDs 1, 4, 5, 12, 13, 14, 15 and 19 are absent on purpose** and must never be
sent. Choosing lord `0` for every march produced server status `256` after the
first attack; walking the pool instead gave five consecutive marches with zero
rejections.

---

## 5. Kingdom movement and mode switching

`[capture]` **There is no kingdom-switch packet.** Verified against a live capture of
login → green map → sand map (`bot/account_data/ventrilo/logs/gge_20261003_12340*.log`):

1. `gbl` is sent with an **empty payload `{}`**. It is not a selector — it reloads
   the map context.
2. The kingdom is chosen purely by the `KID` field in the **`gaa` tile requests**.
   There is no preceding "switch kingdom" command. Changing kingdom is just
   re-issuing the viewport with a different `KID`.
3. `upt {}` is sent once between `gbl` and the `gaa` batch (map update request).
4. The `gbd` bootstrap carries `jca {"CID":-1,"KID":0}` — the initial join, KID 0.

Observed sequence when entering a map (green, KID 0):

```
12:34:13.592 C>S gbl  {}
12:34:13.650 C>S upt  {}
12:34:13.656 C>S gaa  {"KID":0,"AX1":494,"AY1":390,"AX2":506,"AY2":402}
12:34:13.660 C>S gaa  {"KID":0,"AX1":507,...}   (6 tiles: 494/507/520 x 390/403)
```

Same client, sand (KID 1), ~3 s later:

```
12:34:16.360 C>S gbl  {}
12:34:16.416 C>S gaa  {"KID":1,"AX1":572,"AY1":598,"AX2":584,"AY2":610}
12:34:16.446 C>S gaa  {"KID":1,"AX1":585,...}   (6 tiles: 572/585/598 x 598/611)
```

### Viewport origin is **kingdom-specific**

`[capture]` The 3×2 tile grid always steps 13 tiles, but the offset from the castle
is not constant:

| Kingdom | Main castle | Origin observed | Offset |
|---|---|---|---|
| Green (KID 0), area type 1 | `507,403` | `494,390` | **−13, −13** |
| Sand (KID 1), area type 12 | `593,613` | `572,598` | **−21, −15** |

Both rows are confirmed against real data, and the offsets genuinely differ — a
single formula cannot satisfy both. The offset is therefore treated as per-kingdom
data in `session.rs::viewport_offset`, with a test pinning each row. Do not collapse
them back into one constant.

Other mode facts:

| Mode | Established |
|---|---|
| Map mode | Attacking happens here. Map load is `gbl` → `upt` → `gaa` × viewport. |
| Castle mode | Entering a castle switches context; recruitment happens here. |
| Kingdom switch | Not a packet — see above. |
| Castle switch floor | ≥ 3 s, and the map viewport must be re-requested afterwards. |

---

## 6. Timing invariants

`[python]` These are safety properties, not tuning knobs. Weakening them risks
server-side detection or an outright ban.

| Invariant | Value |
|---|---|
| Minimum gap between any two `cra` packets | **4.0 s, transport-wide and persisted across restarts** |
| Jitter added to the `cra` floor | +0.15…0.65 s, always *above* the floor |
| Effective `cra` deadline | `max(now + U(5.5, 9.0), last_cra + 4.0 + jitter)` — the later wins |
| Minimum castle switch | 3.0 s |
| `adi` → `cra` | 5.5…9.0 s (Sands), 3.0 s target (Berimond) |
| Tool-to-troop ratio | **6 troops per tool.** 5 ladders per 30 troops accepted; 7 → error 5 |
| Hourly `cra` cap | 3 per hour by default |
| Max consecutive errors | 2 → stop |
| Commander return hold | 10…20 s |
| RBC target lease after an attack | **720 s** (`TARGET_RESERVE_SECONDS`) |
| Retry after a failed `adi` | 18…44 min (no usable lord), 21…53 min (server error), 2.5…4 h (wrong level) |

`[capture]` The 720 s lease is not optional. Without it a radius scan re-picks the
same nearest tiles on every pass, so one square absorbs the whole run. Ported as a
lease in `empire-hunt` (`target_until`, 720 s on pick) plus the failure-specific
releases above.

`[code]` The Rust `TimingGate` (`empire-core/src/scheduler.rs`) already enforces
the 4 s `cra` floor and the 3 s castle-switch floor, and has tests for both.

`[user]` Reported: a **1 minute cooldown before live state can be re-checked**.
Not yet verified, and the scope is not pinned down — confirm whether it is
per-account, per-target or per-kingdom before encoding it as a floor, because the
scan's batch pacing depends on the answer. Do not add it to `TimingPolicy` until
then.

---

## 7. Server error codes

`[python]` Observed codes and what they mean:

| Code | Meaning | Response |
|---|---|---|
| `0` | Success | — |
| `5` | Action could not be performed (tool ratio) | Do not retry with more tools |
| `95` | Target is not attackable / stale target row | Re-scan; do not retry the same tile |
| `203` | Target already defeated (Berimond) | Retarget |
| `256` | Lord in use `[capture]` | Pick a different commander from the usable pool, and hold that one until its march returns |
| `313` | Not enough troops | Refill, or stop |

`[capture]` Status `95` was seen once on `adi` for a tile the map had reported as a
level 61 RBC (`SERVER_ERROR command=adi status=95`); the same run then succeeded on
the next candidate. Treat it as "skip this tile", not as a fatal error.

`[capture]` Status `256` is **not** fatal. It means the chosen lord was already
marching. Because the commander pool is walked in order and each lord is held until
its own return is seen, a run over five consecutive marches recorded zero `256`s —
if it reappears, the bug is in the commander bookkeeping, not in the envelope.

---

## 8. Capture checklist for anything missing above

1. Start the Python listener (`mitmdump -s bot/rbc_proxy_listener.py`) so captures
   are written under `bot/account_data/<account>/`.
2. Perform the action once, by hand, in the browser.
3. Find the frame in the capture: `%xt%%EmpireEx_21%<cmd>%...` — the three-letter
   command identifies it.
4. Add a row to the relevant table above with the real payload, mark it
   `[python]`, and note the capture file.

Promote an entry to `[code]` only when the Rust path reproduces it.
