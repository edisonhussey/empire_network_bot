# Module map and ownership

One concern, one owner. If a rule exists in two places, one of them is a bug.

| Layer | Owns | Must not |
| --- | --- | --- |
| `crates/empire-game` | Static game data (troops, tools, kingdoms), attack encoding | Do I/O, touch SQLite, or know about the network |
| `crates/empire-core` | Domain rules, pacing, planning, licence, SQLite `store` (**single source of truth**) | Open sockets or know about HTTP |
| `crates/empire-daemon` | Axum API (`lib.rs`, `plans.rs`, `station.rs`, `dev_shared.rs`), one game session per account (`sessions.rs`, `direct/`), `relay.rs` | Invent rules; it applies the ones in `empire-core` |
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
| `scheduler.rs` | The action scheduler: stochastic waits, their recorded diagnostics, history ring |
| `health.rs` | Rolling-hour response health and the safe pause |
| `dev.rs` | The diagnostic snapshot behind the Development tab |
| `station.rs` | Auto-station state machine: one job per attacked castle, gate, castle holds, commander lease (rules in `empire-core/src/station.rs`) |
| `station_view.rs` | The station's part of the Development snapshot |
| `inventory.rs` | Sends the Overview's `dcl` when the window asks and the connection is free |
| `troops.rs` (daemon, top level) | `GET /v1/troops`: the Troop overview over `empire-core/src/store/troops.rs` |
| `pvp.rs` | Feeds `gbd` identity, `gam` movements and `mrm` retreats to the PVP record (rules live in `empire-core/src/pvp.rs`) |
| `tests.rs` | Unit tests for the above |

## `desktop/src/`

| File | Responsibility |
| --- | --- |
| `api.js` | The only HTTP client for the daemon |
| `account-context.js` | Which account is in view, every account's status and colour, who shares a world |
| `dom.js` | `$` and `element` helpers |
| `format.js` | Pure string formatting (no DOM, no state) |
| `views/ruby-chart.js` | Dashboard chart renderer |
| `main.js` | Remaining shared state and views, still to be split view by view |
