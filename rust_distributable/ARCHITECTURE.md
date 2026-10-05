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
| `mod.rs` | Request/status types, `run` / `run_inner` connection loop, shared constants |
| `automation.rs` | Attack automation state machine |
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
