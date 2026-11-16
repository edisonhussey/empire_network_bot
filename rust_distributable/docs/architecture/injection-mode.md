# Injection mode (design, not built yet)

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

## Decisions to confirm

1. **Account state at attach:** use what the daemon already stored for that account (it must have been initialized once in
   direct mode), or require **Start injection bot** before logging in the browser so the engine sees the login itself.
   Recommended: stored state, refreshed by watching the browser's own `gbd`/`dcl` when they pass.
2. **Human and bot on one socket:** while the bot holds the connection (a recruit or refinery visit, an attack sequence),
   the human's clicks can change the view under it. Recommended: the engine's existing context checks (castle/kingdom before
   `bup`/`crst`) refuse and retry, and the UI says "bot is using the view" while it holds it.
