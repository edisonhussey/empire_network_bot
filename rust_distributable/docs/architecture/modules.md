# Module map and ownership

One concern, one owner. If a rule exists in two places, one of them is a bug.

| Layer | Owns | Must not |
| --- | --- | --- |
| `crates/empire-game` | Static game data (troops, tools, kingdoms), attack encoding | Do I/O, touch SQLite, or know about the network |
| `crates/empire-core` | Domain rules, pacing, planning, licence, SQLite `store` (**single source of truth**) | Open sockets or know about HTTP |
| `crates/empire-daemon` | Axum API (`lib.rs`, `plans.rs`), game transport (`direct/`, `relay.rs`) | Invent rules; it applies the ones in `empire-core` |
| `desktop/src-tauri` | Window and service lifecycle only | Contain business logic |
| `desktop/src` | Presentation: a projection of the API plus forms that submit intent | Compute cooldowns, pick targets, or keep its own state machine |

## `crates/empire-daemon/src/direct/`

| File | Responsibility |
| --- | --- |
| `mod.rs` | Request/status types, `run` (reconnect supervisor) / `run_inner` (one socket session), shared constants |
| `automation.rs` | Attack automation state machine: handshake phases, tower/commander selection, waiting |
| `recruitment.rs` | Recruitment automation state machine |
| `fortress_discovery.rs` | Fortress boundary discovery walk and its pacing |
| `base_scan.rs` | Outward map-scan pacing and pending-scan record |
| `packets.rs` | Pure builders for outbound `gaa` / heartbeat frames |
| `observe.rs` | Inbound packet observation and diagnostic persistence (packets are tagged with their account) |
| `tests.rs` | Unit tests for the above |

## `desktop/src/`

| File | Responsibility |
| --- | --- |
| `api.js` | The only HTTP client for the daemon |
| `dom.js` | `$` and `element` helpers |
| `format.js` | Pure string formatting (no DOM, no state) |
| `views/ruby-chart.js` | Dashboard chart renderer |
| `main.js` | Remaining shared state and views, still to be split view by view |
