# Observed game mechanics

This file records behavior visible in captured network traffic. It separates
observations from implementation choices and does not treat UI expectations as
protocol facts.

## Login and initial castle load

The 2026-10-02 20:45:20 Ventrilo capture begins with a new game WebSocket. The
observed order was:

1. Client sends the XML `verChk` system frame. Server returns `apiOK`.
   A direct WebSocket opened without the game-page `Origin` still received HTTP
   status 101, but did not advance to `apiOK`. The successful direct connection
   supplied `Origin: https://empire.goodgamestudios.com`. Incoming protocol
   frames must be accepted as either WebSocket text or UTF-8 binary messages.
2. Client sends the XML `login` system frame with zone `EmpireEx_21` and a value
   derived from the active client version and language.
3. Server sends `nfo`, `core_nfo`, and `rlu`. The `nfo` payload identifies the
   active data version; `rlu` precedes room selection.
4. Client sends XML `autoJoin`. Server sends `joinOK`.
5. Client sends XML `roundTrip` and XT `vck`. The server acknowledges `vck` and
   reports version strings.
6. Client sends `lli`. Two authentication forms are known: password login puts
   the account password in `PW` and sends a null `LT`; portal login puts a live
   session token in `LT` and sends a null `PW`. `RCT` is optional. Portal tokens
   are short-lived and must not be copied from an old capture or persisted in
   diagnostic storage.
7. The server response `%xt%lli%1%0%` is the positive authentication marker.
   It has no JSON payload; the final `0` is the response status. Any non-zero
   status is a rejection and does not prove login.
8. The server emits the account bootstrap stream. `gbd` contains the player's
   castle inventory and appeared before the client's initial castle requests.
9. The client requests `gbl`, `jca {"CID":-1,"KID":0}`, `alb`, `sli`, `gie`,
   `asc`, `sie`, `ffi`, and `kli`. The server's successful `jaa` response is the
   positive observation that a castle snapshot was loaded. With `CID=-1`, the
   server resolved the initial Green castle.
10. A castle `jaa` response contains the active castle row in `gca.A`, stored
    resources in `grc`, and current castle inventory in `gui.I`. Inventory is a
    list of `[item_id, quantity]` pairs; crossbowmen are item ID `607`.

The Rust state machine uses response markers (`apiOK`, `rlu`, `joinOK`, `vck`,
`gbd`, and `jaa`) rather than fixed sleeps to advance these stages.

The Python helper and PyGGE were used only as independent evidence for the
ordering and field meanings. The Rust implementation owns its packet encoder,
parser, WebSocket lifecycle, and state machine and has no PyGGE or Python
runtime dependency.

On 2026-10-03, the standalone Rust client completed this sequence against the
Ventrilo account using password authentication. A later proof run continued
through the account bootstrap, main-castle `jaa`, six Sands `gaa` map tiles, and
an explicit `jca` into the Sands castle learned from `gbd`. It then read the
Sands castle's `jaa` inventory and closed without sending recruitment or attack
commands.

## Castle view, map view, and kingdoms

Opening the general map is primarily a local client UI transition. No dedicated
"close castle" network command was observed. Once the map was visible, the
client emitted `gbl`, `upt`, and groups of `gaa` reads.

`gaa` supplies the positive network evidence for map context:

- `KID=0` requests and responses were observed for the Green kingdom map.
- `KID=1` requests and responses were observed after moving to Burning Sands.
- A map tile is requested as an inclusive 13-by-13 rectangle (`AX1`, `AY1`,
  `AX2`, `AY2`). Adjacent tiles advance by 13 coordinates.
- The captured Sands viewport began at `572:598` and requested three columns by
  two rows. Those coordinates are account/view dependent and are configurable.

A successful Sands `gaa` response is therefore the current definition of
"Sands ready" for a network-only client. It does not imply that a separate game
webview has visually changed.

## Stable level and attack travel options

The `HBW` field in `cra` is not one universal coin-horse identifier. Captured
Ventrilo packets from a level-3 stable established this sequence:

- no horse: `HBW=-1`, `PTT=0`
- coin horse: `HBW=1007`, `PTT=0`
- first ruby horse: `HBW=1008`, `PTT=0`
- second ruby horse: `HBW=1009`, `PTT=0`
- feather travel: `HBW=-1`, `PTT=1`

The identifiers form three observed stable families: level 1 uses
`1001–1003`, level 2 uses `1004–1006`, and level 3 uses `1007–1009`. This is a
property of the source castle, not of the account or kingdom. In particular,
Pingpoko's Sands castle advertised `[1004,1005,1006]`; a `cra` sent from it with
the previously hard-coded `1007` was rejected, while the real client used
`1004` successfully.

The account bootstrap exposes the authoritative per-castle list at
`gbd.gpc.A[]`: `AID` is the castle id, `KID` is its kingdom, and `UH` is its
unlocked HBW list. The list order is not the travel-mode order (a captured
level-3 castle returned `[1008,1009,1007]`). Across the observed families the
coin choice is the lowest positive `UH` value. OpenAuto therefore stores this
data for each castle and resolves coin `HBW` from the attack's exact source
castle immediately before constructing `cra`.

Tasks store a human travel mode rather than a castle-specific number. After
sorting the source castle's three positive UH values, OpenAuto maps them as
coin = UH[0], ruby 1 = UH[1], and ruby 2 = UH[2]. Feather is the separate wire
mode HBW=-1, PTT=1. Coin and both ruby choices use PTT=0. This mapping is
performed at send time so a task can retain the same semantic choice when used
with castles at different stable levels. If no initialized castle matches the
source kingdom and coordinates, no cra is sent; guessing a global HBW is known
to cause server rejection.

## What remains unknown

- How the portal obtains or refreshes `LT` has not yet been isolated into a
  stable request sequence. This does not block direct password authentication.
- The bootstrap stream contains many server pushes. Only the markers required
  by the demonstrated path are interpreted so far.
- No claim is made that every account resolves `CID=-1` to the desired castle.
  Explicit castle selection can be added once the castle inventory parser is
  promoted into the direct-session planner.
- A running account mode is executed by the direct session after Sands is
  ready. The runner leases a database target, sends `adi`, selects an available
  commander from that task's allocation, waits the configured jitter, then
  sends `cra`. It does not depend on the visible client screen.
- The four-second `cra` separation is a hard transport-level floor. Normal
  waits add variance above it. An acknowledged march is written immediately;
  commander availability and the target's last-attack time are durable.
- Stopping an account mode prevents a pending `cra` from being committed. A
  disconnected socket cannot execute a mode even if its database flag is on.
- Account discovery and map targets persist locally. A normal login for an
  initialized account reuses that data and skips map refresh packets; an
  explicit additional scan reconnects with a chosen radius and requests only
  windows that are not still fresh.
- Network logs can continue to show stored history while disconnected, but
  they are live only while the direct game socket is connected. Closing or
  losing that socket stops the account's running mode.
- A successful `cra` supplies the outbound duration. Until a battle result is
  observed, commander availability uses `1.2 × outbound + 5–10 seconds`; this
  is the restart-safe fallback if the socket closes before the result arrives.
  A matched `cat` replaces that estimate with its exact remaining return
  duration plus a randomized 5–10 second reuse hold.
