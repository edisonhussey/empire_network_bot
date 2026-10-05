# Architecture

## The processes

Two Rust processes and one JavaScript document.

```
desktop/ (Tauri 2 window, vanilla JS in src/main.js)
   │  spawns and reuses
   ▼
empire-desktop --service        ← owns the SQLite database and the game socket
   │  serves http://127.0.0.1:47821/v1
   ▼
empire.sqlite3  (~/Library/Application Support/com.openauto.desktop/)
```

- **The window is disposable.** Closing it leaves the service running; reopening
  reuses the live service so there is no re-login. That reuse is why
  `empire_daemon::API_VERSION` must be bumped for **any** behaviour change: a
  stale service is otherwise indistinguishable from a current one, and new code
  silently does nothing. `ensure_service()` compares the constant against
  `/v1/health` and kills a mismatch.
- **One database, one path.** `empire_core::paths::data_dir()` is the only
  place that decides it. Two processes opening the same database through
  different paths means two `-wal` files and corruption.
- **No reusable login token.** The login reply is empty, so a session survives
  only by keeping the socket open. "Connect once" is an architectural property.
- **A service swap drops the session** (`transport_connected` goes false). After
  installing a new build, reconnect from Start.

## The API

Everything the window and the lab panel can do goes through these:

| route | method | licence feature | purpose |
|---|---|---|---|
| `/v1/health` | GET | — | api version, service pid, whether a socket is connected |
| `/v1/licence` | GET/POST | — | activation state; POST to apply a token |
| `/v1/messages` | GET | `game_network` | the recorded packet log (`?limit=`) |
| `/v1/injections` | POST | `game_network` | queue a raw `%xt%…%` packet onto the live socket |
| `/v1/direct` | GET/POST | `game_network` | session phase, bot state, connect/disconnect |
| `/v1/dashboard`, `/v1/hunt` | GET | `account_initialize` | aggregated run views |
| `/v1/accounts` | GET/POST | `account_initialize` | stored accounts; POST initialises one |
| `/v1/plans/*` | GET/POST | `account_initialize` | attack bots, tasks, recruit bots |
| `/v1/relay` | GET (ws) | `game_network` | socket relay |

`POST /v1/injections` takes `{"packet": "...", "ttl_ms": 15000}`. The packet is
parsed before it is queued, and the queue is 64 deep. It fails with
`no active relay transport` when nothing is connected.

## Initialisation, end to end

1. **Sign in.** `gbd` arrives with the account's castles and kingdoms.
2. **Map scan.** For each kingdom the account enabled, a grid of `gaa` tiles of
   radius `2r+1` around that kingdom's castle. `radius 50` → an 8×8 grid of
   64 tiles, covering ±52 map units. This is what fills `map_scan_window` and
   the "areas learned" figure.
3. **Fortress sweep.** One cursor per (kingdom, lattice grid) walks the whole
   kingdom. See `fortress-discovery.md`.
4. **Automation.** Only starts once the sweep reports complete. While it runs,
   nothing attacks — this is why windows can be missed, and it is one of the
   entries in `open-defects.md`.

## Where state lives

| table | holds |
|---|---|
| `account_profile`, `owned_castle`, `account_commander` | what the account is |
| `map_scan_window` | which map tiles have been learned |
| `fortress_scan_state` | one row per (account, kingdom, grid): sweep bounds + cursor |
| `fortress_target` | every fortress seen, its cooldown and when it frees |
| `rbc_target` | every robber baron inside the configured radius |
| `commander_state` | which commanders are busy and until when |
| `network_message` | the packet log — a **rolling buffer**, older rows age out |
| `task_definition`, `task_subscription`, `automation_mode*` | what the bot is told to do |

`fortress_scan_frontier` also still exists and is **not** a bug: it is the
retired per-coordinate work queue, kept because the declarative schema list is
re-applied on every launch and migrations V13/V14 still reference it. Nothing
reads it.

## Building and installing

```sh
cd rust_distributable/desktop && npm run release
pkill -f 'OpenAuto.app/Contents/MacOS'
rm -rf /Applications/OpenAuto.app
cp -R src-tauri/target/release/bundle/macos/OpenAuto.app /Applications/
open /Applications/OpenAuto.app
curl 127.0.0.1:47821/v1/health   # confirm api_version
```

Bump `API_VERSION` in `crates/empire-daemon/src/lib.rs` and the version in
`desktop/src-tauri/tauri.conf.json` + `Cargo.toml` first, or the window will
keep talking to the old service.

Verify a UI change landed by looking in `desktop/dist/assets/*.js` — Tauri
embeds the frontend **compressed**, so grepping the installed binary for UI text
always finds nothing.

## The lab

`development/lab/server.py` serves a control panel on `127.0.0.1:8799` and
forwards to the daemon through an allowlist and a two-per-two-seconds gate.
It exists so questions about the live server can be answered without a rebuild.
