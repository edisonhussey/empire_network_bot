# Sequence: login and the initial castle load

The order of frames from a new game WebSocket to a loaded castle, as the Rust client does it. Facts were observed in
the 2026-10-02 Ventrilo capture and proven by the standalone client on 2026-10-03. The state machine advances on the
**response markers** (`apiOK`, `rlu`, `joinOK`, `vck`, `gbd`, `jaa`), never on fixed sleeps
(`empire-core/src/session.rs`). Individual packets: [`../README.md`](../README.md); the server header and `lli` details are
in [`../../architecture/network_requests.md`](../../architecture/network_requests.md).

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

## Open questions

* How the portal obtains or refreshes `LT` is not isolated into a stable request sequence. This does not block direct password login.
* The bootstrap stream has many server pushes; only the markers the demonstrated path needs are interpreted.
* Not every account is known to resolve `CID=-1` to the desired castle. Explicit castle selection can be added once the castle inventory parser is promoted into the planner.
