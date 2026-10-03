# Task 1 — standalone Rust login proof

## Result

`empire-login` is a network-only Rust executable. It connects directly to the
Goodgame Empire TLS WebSocket and does not start the daemon, proxy, database,
desktop UI, browser, or any Python process. After authenticating, it proves the
main-castle load, loads six Sands map tiles, enters the learned Sands castle,
and reports selected live state from the castle response.

The reconstructed sequence is response-driven:

```text
TLS WebSocket
  -> XML verChk
  <- apiOK
  -> XML login
  <- rlu
  -> XML autoJoin
  <- joinOK
  -> XML roundTrip + XT vck
  <- vck acknowledgement
  -> XT lli (password or portal token)
  <- XT lli status 0: authenticated
```

The proof condition is deliberately narrow. A socket connection alone is not a
login. The program reports success only when it parses the game's `lli`
response and its status is exactly `0`:

```text
LOGIN_PROOF command=lli status=0 authenticated=true
```

## Evidence used

- The Ventrilo capture from 2026-10-02 records the browser handshake and the
  payloadless successful response `%xt%lli%1%0%`.
- The existing Python session helper confirms the password form (`PW` set,
  `LT` null) and treats `lli` status `0` as success.
- PyGGE independently confirms the system-frame ordering and password/token
  field behavior. It is a research reference only; no PyGGE source, dependency,
  class structure, or runtime is used by this executable.

## Verification

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release -p empire-login
```

The protocol regression suite includes the captured payloadless `lli`
acknowledgement, because interpreting its last field as a request ID instead of
a status would produce a false negative after a real successful login.

## Live proof

The optimized standalone binary was run once against the Ventrilo account on
2026-10-03. Sensitive fields and the outbound login packet were not logged. Its
sanitized output was:

```text
LOGIN_START player=Ventrilo endpoint=wss://ep-live-us1-game.goodgamestudios.com/ auth=password timeout=30s
SOCKET_OK http_status=101 Switching Protocols
PHASE SocketHandshake
PHASE AwaitingRoom
PHASE AwaitingVersion
PHASE Authenticating
PHASE Authenticated
LOGIN_PROOF command=lli status=0 authenticated=true
```

The extended live proof produced:

```text
ACCOUNT_PROOF owned_castles=6 main_castle_id=16011862 sands_castle_id=16366514
MAIN_CASTLE_PROOF kingdom_id=0 castle_id=16011862 name="._." coordinates=509:405
SANDS_MAP_PROOF kingdom_id=1 tiles_loaded=6
SANDS_CASTLE_PROOF kingdom_id=1 castle_id=16366514 name="Castle Ventrilo" coordinates=593:613
SANDS_STATE_PROOF crossbowmen=13753 food=566709 inventory_entries=20
FULL_PROOF authenticated=true main_castle=true sands_map=true sands_castle=true inventory=true
```

This is live server evidence, rather than a replay or unit-test fixture. These
counts are a snapshot and will change with gameplay. The client closed after
the final Sands castle response and sent no recruitment or attack commands.
