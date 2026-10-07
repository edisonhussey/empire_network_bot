# Architecture decisions

## Tauri over Electron

The brief mentioned both. Tauri is used because it satisfies the explicit Rust
framework requirement and uses the operating-system webview instead of bundling
another Chromium runtime. That directly supports the minimum-RAM objective. The
local API boundary means an Electron UI can still replace Tauri without changing
the network or persistence crates. In the distributable build, Tauri embeds the
daemon library in the same process; the standalone daemon binary exists only as
a development and network-testing entry point.

## Bounded data everywhere

| Resource | Bound |
| --- | ---: |
| SQLite protocol messages | 200 rows |
| Pending injection queue | 64 packets |
| SQLite connections | 1 |
| Accepted XT packet | 4 MiB |
| Injection lifetime | 1–60 seconds |
| `cra` packet floor | 4 seconds |
| Castle-switch floor | 3 seconds |

The daemon does not create per-packet files. Structured SQLite rows are both
developer-visible and suitable for migration in a packaged executable. Payloads
larger than the packet limit are represented by small metadata records rather
than copied into persistent storage.

## Security boundary

The daemon binds to loopback and permits only the known development and packaged
Tauri origins. A later hardening pass can replace HTTP injection with Tauri IPC
or a per-launch bearer capability. Licence verification uses Ed25519: the
private key remains in an external issuer, while the application contains only
the public verification key. Claims contain `not_before` and `expires_at`
epochs.

## Licence trust chain

```text
offline master private key
        │ signs exact payload bytes
        ▼
OA1.<base64url payload>.<base64url Ed25519 signature>
        │
        ▼
embedded public-key ring → verify signature → parse JSON → check time/revision/features
        │
        ├── invalid: activation UI only; no game network capability
        └── valid: explicitly entitled features may run
```

There is one primary issuer key, not one keypair per installation. All builds
carry the same public key. The keyring abstraction exists solely for controlled
future rotation. Renewal is a higher signed revision of a licence, which avoids
replayable unsigned “credit” counters.

## Runtime flow

```text
Tauri process
├── webview UI ──loopback API──┐
└── embedded Rust daemon <─────┘──direct client / relay──> game server
        │
        ├── response-driven login/navigation state machine
        ├── bounded injection queue
        └── SQLite (latest 200 messages + durable state)
```

The transparent browser proxy and direct-login client are adapters on the left
and right edges of this flow; neither is allowed to duplicate core state logic.

Task definitions are event descriptions, while task subscriptions are separate
data mapping task IDs to castle IDs. The timing gate is shared across task types:
it enforces the strict global `cra` floor, adds a castle-switch floor, and applies
smaller bounded variance to background traffic. Socket code does not decide
which castles subscribe to a recruit or attack event.

## Attack scheduling

Attacks are a pipeline with two independent inputs: a **ready tower** and a
**free commander**. The scheduler (`direct/automation.rs`, state in
`empire-core/src/store.rs`) exists to keep both visible, so that when throughput
is low the cause can be read off rather than guessed.

### One handshake at a time

`Idle -> (map open) -> ADI -> wait 2-4 s -> CRA -> ack -> Idle`. Only one
handshake is in flight; the global CRA floor (4 s) and the post-ack pause live in
`empire-core/src/pacing.rs`. That is roughly 3 attacks a minute, enough to keep
~20 commanders busy on a ~9 minute round trip. A refusal or timeout returns to
`Idle`; nothing else mutates the pipeline.

### Towers: trust the server's cooldown, not our guess

* Every RBC row in a map response carries its remaining cooldown
  ([`docs/network/gaa.md`](docs/network/gaa.md)). It is stored as
  `rbc_target.server_free_at_ms` and refreshed by **any** map response, scan or
  not, for towers already known. It never adds a tower the operator did not scan.
* A tower is selectable only when `reserved_until_ms <= now` (lease / local
  cooldown), `server_free_at_ms <= now`, and the ledger cooldown (landing + 3 h,
  capped at sent + 4 h, + 5 min) has passed. `next_rbc_ready_ms` is the minimum
  of the same three, so the two queries cannot disagree.
* Selecting a tower leases it for 12 minutes. A handshake abandoned before the
  attack releases the lease at once (`release_rbc_target`).
* A refusal with status 95 means our map of that tower is stale. The tower is
  held for a minute, its tile is re-read, and the server's real cooldown replaces
  the guess. A flat one-hour park (the old behaviour) wasted every minute the
  tower was actually ready. A refusal the tile cannot explain parks it 30 min.

### Commanders: reserved early, lent when idle

* Identity is the server `LID`; usable LIDs are a curated list
  (`hunt::USABLE_COMMANDER_LIDS`). The `gli.C` list in an ADI reply is the whole
  roster, not availability, and is not used to decide who is free.
* A commander is reserved when the CRA **leaves** (5 minute provisional hold),
  not when it is acknowledged. The ack replaces that with travel time; `cat`
  (matched by target and `LID`, never by `MID`) sets the real return. A rejected
  CRA releases the commander after 15 s.
* A task's commanders are an allocation, not a wall. When a task has a ready
  tower and all its own commanders are out, idle commanders from a task with
  nothing ready are lent to it (`ActiveModeTask::spare_commanders`). A task with
  work always keeps its own.

### Waiting is computed, not polled

With nothing to send, the scheduler takes, per task, the later of "next tower
ready" and "next commander home", sleeps until the earliest (1.5-30 s plus
jitter), and writes the reason to the status line, for example
`Waiting - Sand 61: 14/17 commanders home, next tower in 6m 12s`. *Next tower in
...* is a tower shortage; *tower ready, next commander in ...* is a commander
shortage. Utilisation is capped by roughly `towers / cooldown`: more commanders
cannot raise it, more towers (kingdoms, levels, tasks) can.

### Errors and the safety pause

Two operational errors in five minutes pause attacks until the window clears;
five disable the mode. Target-specific refusals (95) and tile-read timeouts are
not operational errors. A request that times out while nothing at all has
arrived for 25 s is blamed on the link, not the target: the tower is released,
no error is counted, and an attack that may have left keeps its commander
reserved.

## Connection supervision

`direct::run` supervises `run_inner`, which is one socket session. It was
previously fire-and-forget: a dropped connection ended the run silently. On
7 Oct the Mac's Wi-Fi lost its address mid-run (`.110`, back as `.101` 90 s
later), the socket was reset, and the bot sat dead for 50+ minutes.

* Retried: a socket error, a reset, or 90 s without any frame from the server
  (`ConnectionLost`). Delays are 5, 10, 20, 40, 60 s with jitter, for up to
  30 minutes. The learned map is reused, so the bot resumes attacking instead of
  rescanning, and the running mode resumes because it is persisted.
* A session that did not stay up 2 minutes does not reset the give-up clock, so a
  server that keeps dropping us is not hammered.
* Final, never retried: licence refusal, rejected login, and a clean server close
  (which may be the same account logging in elsewhere; the bot does not fight it).
* While reconnecting the status reports `bot_state = "reconnecting"`.
* Marches that land while offline never produce a `cat`, so their ledger rows
  stay `sent` with no loot. They are not reconciled afterwards.

## Module map and ownership

One concern, one owner. If a rule exists in two places, one of them is a bug.

| Layer | Owns | Must not |
| --- | --- | --- |
| `crates/empire-game` | Static game data (troops, tools, kingdoms), attack encoding | Do I/O, touch SQLite, or know about the network |
| `crates/empire-core` | Domain rules, pacing, planning, licence, SQLite `store` (**single source of truth**) | Open sockets or know about HTTP |
| `crates/empire-daemon` | Axum API (`lib.rs`, `plans.rs`), game transport (`direct/`, `relay.rs`) | Invent rules; it applies the ones in `empire-core` |
| `desktop/src-tauri` | Window and service lifecycle only | Contain business logic |
| `desktop/src` | Presentation: a projection of the API plus forms that submit intent | Compute cooldowns, pick targets, or keep its own state machine |

### `crates/empire-daemon/src/direct/`

| File | Responsibility |
| --- | --- |
| `mod.rs` | Request/status types, `run` (reconnect supervisor) / `run_inner` (one socket session), shared constants |
| `automation.rs` | Attack automation state machine: handshake phases, tower/commander selection, lending, waiting |
| `recruitment.rs` | Recruitment automation state machine |
| `fortress_discovery.rs` | Fortress boundary discovery walk and its pacing |
| `base_scan.rs` | Outward map-scan pacing and pending-scan record |
| `packets.rs` | Pure builders for outbound `gaa` / heartbeat frames |
| `observe.rs` | Inbound packet observation and diagnostic persistence |
| `tests.rs` | Unit tests for the above |

### `desktop/src/`

| File | Responsibility |
| --- | --- |
| `api.js` | The only HTTP client for the daemon |
| `dom.js` | `$` and `element` helpers |
| `format.js` | Pure string formatting (no DOM, no state) |
| `views/ruby-chart.js` | Dashboard chart renderer |
| `main.js` | Remaining shared state and views, still to be split view by view |

## Working rules

1. **Contract first.** Agree the types and the API shape before any implementation.
2. **Move, then change.** Refactors move code verbatim; behaviour changes are separate commits.
3. **Verify every step.** `cargo check --workspace --tests`, `cargo test --workspace`, and `npx vite build` in `desktop/` must pass before moving on.
4. **Bump `API_VERSION`** whenever behaviour the window depends on changes.
5. **New responsibility, new module.** If an edit adds a concern to a file, it belongs in its own file.
