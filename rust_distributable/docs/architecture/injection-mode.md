# Injection mode: the bot over the browser's socket

Goal: **Start injection bot** runs the same engine as **Start bot**, but over the game socket of a browser that is already
playing through proxy_packet_monitor, instead of a socket the daemon opens and logs into itself. One code base: every
automation (attack bot, recruit, station, refinery, transfers) is unchanged; only the transport and the login differ.
The standalone bot must keep working exactly as now.

## The seam

`direct/mod.rs::run_inner` today:

```text
empire_socket::connect(endpoint) ──▶ (sink, stream) ──▶ SessionMachine::on_connected() (login frames)
                                                   └──▶ select! loop: stream → observe/automations; ticks → sink
```

Injection mode swaps exactly two things:

| | Direct (today) | Injection |
| --- | --- | --- |
| Transport | `empire_socket::connect` → tungstenite `(sink, stream)` | `ProxyLink` → channel-backed `(sink, stream)` with the same item type |
| Login | `SessionMachine` drives `vck` / `lli` / `gbd` | none: the browser is logged in. The machine starts at `SandsReady` from the account's stored bootstrap (castles, commanders) |
| Heartbeat (`pin`) | the daemon sends it | the browser sends it; the daemon does not |
| Reconnect | supervisor reconnects | a lost proxy link ends the session; the browser owns the socket |

So `run_inner` becomes generic over its sink and stream (`Sink<Message>` + `Stream<Item = Result<Message, _>>`), and
the two differences above become a `TransportMode` flag checked in three places (login frames, heartbeat, reconnect).
Everything between stays the same code.

## The proxy link

```text
browser ⇄ mitmproxy (proxy_packet_monitor addon) ⇄ game server
                 │  every game frame, both directions          ▲
                 ▼                                              │ frames to send to the server
        daemon  GET /v1/attach?account=<id>  (WebSocket) ───────┘
```

* The addon opens the WebSocket when the game socket opens and pushes `{"dir":"s2c"|"c2s","text":…}` for each frame.
* Server frames feed the engine's stream as if they came from its own socket. Client frames are observed too, so the
  engine sees the human's navigation (`jca`, `gaa`) and does not fight it.
* The daemon's sink writes `{"inject":…}` back; the addon sends it with `inject.websocket` (what the Inject box and
  **Test refine** already do).

## Decided (2026-11-16) and built

1. **Account state comes from the database.** The account must have logged in once with **Start bot**: that login stores its
   identity (`app_state` `session.identity.<account>`), castles and commanders. Attaching reads them, binds the licence and
   starts at `SandsReady`. Everything the browser's traffic carries (navigation, stock, commanders, marches) keeps updating
   the same tables as in direct mode.
2. **The person and the bot share the socket.** A view command the person sends (`jca`, `gaa`, `cra`, `adi`, `bup`, `crst`,
   `kut`; `direct/pause.rs`) makes every bot step aside for **one minute**: a step in flight is abandoned as a failure (an
   attack target not yet attacked goes back to the pool, an attack already sent keeps its lease), the session keeps
   listening, and the bots resume from the live state when the minute is up.
3. **Pause button** (both modes, Control Panel): the same, without the minute: bots stop sending until **Resume**, the socket
   stays and every frame is still read. Nothing is cleared or rewritten.

## How to run it

1. Start the proxy in proxy_packet_monitor and play in the browser through it.
2. Type the OpenAuto account name next to **Start injection bot** and press it (the account must exist in OpenAuto).
3. In OpenAuto, the account shows "via proxy (injection bot)". Choose and start the bots in the Control Panel as usual.
4. **Stop injection bot**, closing the game tab or stopping the proxy ends the session; the game is not touched.

## Where the code is

| Concern | File |
| --- | --- |
| The two transports behind one sink/stream | `crates/empire-daemon/src/direct/link.rs` |
| Pause and the one-minute human hold | `crates/empire-daemon/src/direct/pause.rs`; `interrupt` on `Automation`, `RecruitmentAutomation`, `RefineryAutomation` |
| The `/v1/attach` link and `POST /v1/direct/pause` | `crates/empire-daemon/src/attach.rs` |
| Login skipped, heartbeat skipped, no reconnect, identity read | `crates/empire-daemon/src/direct/mod.rs` (`is_attached`) and `run` |
| The proxy side (wsproto client, forward, inject) | `proxy_packet_monitor/backend/openauto_link.py`, `incoming_addon.py` |

The direct path is unchanged apart from going through `link::direct` (the same socket, boxed).
