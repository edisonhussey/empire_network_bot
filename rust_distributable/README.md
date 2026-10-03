# Empire Network — Rust distribution

This directory is a clean-room Rust replacement path for the Python proxy bot.
It deliberately shares no runtime files with the Python implementation.

## What works now

- `empire-core` parses and encodes XT packets, owns typed network events,
  validates expiring Ed25519 licences, and persists state in SQLite.
- SQLite retains the newest 200 protocol messages. There is no unbounded raw-log
  folder and the pool uses one connection to keep memory predictable.
- `empire-daemon` exposes a local-only API and a WebSocket relay. Packets can be
  injected into the active relay through `POST /v1/injections`; inputs are XT
  validated, TTL-bound, and pass through a bounded 64-item queue.
- `desktop` is a Tauri 2 application with a small dependency-free UI for daemon
  health, recent messages, and controlled packet injection. It embeds and starts
  the Rust daemon in-process, so the packaged application has no Python runtime
  or separately managed service.
- `empire-core` includes a response-driven direct-session state machine for the
  observed socket handshake, authenticated `lli` login, main-castle bootstrap,
  and a configurable six-tile Burning Sands viewport. The daemon can run it on
  its own WebSocket without a browser relay.
- `empire-login` is the Task 1 proof binary. It has no daemon, database, UI,
  proxy, or Python dependency: it opens the game WebSocket directly, performs
  the reconstructed login sequence, loads the main castle and Sands map, enters
  the account's learned Sands castle, and reports a sanitized live-state proof.

## Honest boundary

The current relay is native and functional, but the existing Goodgame browser
does not automatically route its established TLS WebSocket through it. The next
adapter is a target-scoped HTTP CONNECT/TLS interception layer with a locally
installed CA. Initial account login and automatic Green-to-Sands navigation are
implemented from the captured game-socket sequence. Login accepts either the
account password or a fresh portal `LT`; `RCT` is optional. Acquisition and
refresh of portal tokens remains deferred until its HTTP exchange is captured.
The remaining proxy adapter is isolated from the database, scheduler,
licensing, UI, and direct-session core.

## Layout

```text
crates/empire-core     protocol, events, bounded injection, SQLite, licensing
crates/empire-daemon   local API and upstream WebSocket relay
crates/empire-login    standalone network-only login proof
desktop                Tauri/Vite distributable application shell
network_only           protocol research notes
proxy                  notes for the transparent CONNECT adapter
```

## Run the standalone login proof

The binary can read the existing `[gge]` section without modifying the file:

```sh
cd rust_distributable
cargo run --release -p empire-login -- --config /path/to/account.ini
```

Environment variables override the file. The minimum environment-only form is:

```sh
GGE_USERNAME='account-name' \
GGE_ACCOUNT_ID='portal-account-id' \
GGE_PASSWORD='account-password' \
cargo run --release -p empire-login
```

`GGE_LOGIN_TOKEN` may replace `GGE_PASSWORD`. Optional overrides include
`GGE_SERVER_URL`, `GGE_SERVER_HEADER`, `GGE_RCT`, `GGE_CLIENT_VERSION`,
`GGE_CONM`, `GGE_RTM`, and `GGE_LOGIN_TIMEOUT`.

Successful evidence ends with server-derived state like this:

```text
LOGIN_PROOF command=lli status=0 authenticated=true
SANDS_MAP_PROOF kingdom_id=1 tiles_loaded=6
SANDS_CASTLE_PROOF kingdom_id=1 castle_id=... name="..." coordinates=...:...
SANDS_STATE_PROOF crossbowmen=... food=... inventory_entries=...
FULL_PROOF authenticated=true main_castle=true sands_map=true sands_castle=true inventory=true
```

The program never prints credentials or the login packet. It makes one login
attempt, derives castle IDs from the server's `gbd` response, closes after the
full proof, and does not retry.

## Run the daemon

```sh
cd rust_distributable
cargo run -p empire-daemon
```

Defaults:

- API: `127.0.0.1:47821`
- data: `rust_distributable/data/empire.sqlite3`
- overrides: `EMPIRE_BIND`, `EMPIRE_DATA_DIR`, and `RUST_LOG`

Useful endpoints:

```text
GET  /v1/health
GET  /v1/messages?limit=50
POST /v1/injections       {"packet":"%xt%gbl%1%{}%","ttl_ms":15000}
GET  /v1/direct           current direct-session status
POST /v1/direct           open login-to-Sands direct session
DELETE /v1/direct         close the direct session
GET  /v1/relay?upstream=wss://example.invalid/socket   (WebSocket upgrade)
```

## Run the desktop application

The desktop process starts the daemon itself and stores SQLite data in the
operating system's application-data directory:

```sh
cd rust_distributable/desktop
npm install
npm run tauri dev
```

The standalone daemon remains available for network tests that do not require a
GUI. Do not run it on the default port at the same time as the desktop app.

## Design constraints

- Bind locally unless the user explicitly changes the address.
- Never keep an unlimited packet history in memory or on disk.
- Never place private signing keys in the distributed application. The app
  embeds only an Ed25519 public key and verifies time-bound signed claims.
- Route all injected packets through one bounded queue and one active transport.
- Keep game-specific state machines out of UI code.
- Do not claim a protocol transition is implemented until observed traffic
  proves it.

## Verification

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
