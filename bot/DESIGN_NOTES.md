# Bot architecture and design notes

## Module map

| Module | Role |
| --- | --- |
| `bot/packets.py` | Protocol layer. `xt` framing plus **pure** parsers (`parse_xt_packet`, `extract_raw_packet`, `adi_level`, `sands_level_from_gaa_value`) and constants (`SAND_SERVER_HEADER`, `AREA_BARRON`, `MAP_CHUNK_SIZE`, `HBW_VALUE`). No I/O, no mutable state. |
| `bot/attacks.py` | Every attack payload as a visible, writable `Attack` object, plus `ATTACK_REGISTRY`. This is where you change *what gets sent*. |
| `bot/tasks.py` | The task plan: which kingdom, which RBC levels, how many commanders, which attack. Commander numbers are handed out in task order. |
| `bot/scheduler.py` | The scheduling engine only: `Task`, `TaskDefinition`, `Scheduler`, commander-number → LID mapping, kingdom rotation, greedy allocation. |
| `bot/accounts.py` | Account registry driven by `credentials/*.env`. Dynamic `--<username>` flags, `Account` records, per-account paths. |
| `bot/db.py` | Database connection and schema. `ensure_schema`, `init_database`, `table_counts`. |
| `bot/cli.py` | Single operator entry point (`python -m bot.cli ...`). |
| `bot/bot.py` | Core runner: attack sending, commander/target reservation, proxy control-file transport. |
| `bot/proxy_bot.py` | Proxy driver (sands/storm/auto). |
| `bot/sands_proxy.py` | Sands-mode analytics and control. |
| `bot/rbc_proxy_listener.py` | mitmproxy listener; drives packets inside the live session. |
| `bot/populate_database_rbc.py` | Loads RBC targets into the DB from mitmproxy capture logs. |
| `bot/storm_database.py` | `storm_target` table and storm-specific helpers. |
| `bot/db_account.py` | Adds/repairs the `aid` column and composite keys. |
| `bot/game_data/` | Troop/tool/kingdom catalogues and the in-house `Attack`/`side`/`wave` builders. |

Removed / relocated:

* `bot/sand_rbc_farm/` — the standalone state loop was dead code (no scheduler or
  runner invoked it). Parsers → `bot/packets.py`, attacks → `bot/attacks.py`,
  state files deleted (Postgres owns state). Parser compatibility is covered by
  the fact that its helpers were already the only thing imported.
* `bot/storm_scan_loop.py`, `bot/storm_proxy_control.py`, `bot/setup_storm_database.py`,
  `bot/send_gaa_probe.py` — orphaned legacy modules, moved to `unused/bot_legacy/`.
  `setup_storm_database.py` is superseded by `python -m bot.cli db init`.

## Multi-account model

* Accounts are discovered from `credentials/*.env` (`USERNAME`, `PASSWORD`, `SERVER`, `AID`).
* Adding an account is a **data** change: drop in a new `.env` file and its
  `--<name>` flag appears automatically.
* One shared database (`empire_bot`), isolated per account by the `aid` column.
  Tables carry `(aid, ...)` primary keys / unique indexes.
* Per-account runtime state lives under `bot/account_data/<aid>/`:
  `proxy_control.json`, `rbc_proxy_listener.log`, `logs/`, `latest_logs/`.
* `--<username>` works in any position, including on the legacy entry points
  (`bot/bot.py`, `bot/proxy_bot.py`, `bot/populate_database_rbc.py`).

## Login detection and the account guard

Making the account flag optional is what *lets* you forget it, so the flag stays
and is instead **verified**. The identity is the **login name** (``NOM``):

1. The `lli` login handshake carries `NOM` and `AID`. **`AID` is unusable as an
   identity**: it is a shared portal account id, identical for every game account
   under one login. Measured on real captures, `NOM='Ventrilo'` and
   `NOM='pingpoko'` both reported `AID=1782860727866351909`. `NOM` is the only
   discriminator in the packet. `login_identity_from_packet()` returns
   `{name, portal_account_id}` and never the `LT` / `RCT` tokens.
2. `bot/rbc_proxy_listener.py::detect_login_account()` resolves the login name to
   an account, writes `bot/account_data/session.json`, rebinds capture/control
   paths to that account, and runs `bot/db.py::register_account(key)`. A login
   name never seen before gets `credentials/<name>.env` created automatically, so
   a brand new account appears in the tables on first login with no setup.
3. `Account.key` is the canonical scope key and equals the login name. It keys
   the data directory (`bot/account_data/<key>/`) and the `aid` column in every
   table (`aid` is TEXT). `Account.aid` is an alias for `Account.key` so the
   existing SQL did not need rewriting.
4. `bot/accounts.py` exposes `read_session`, `session_account` and
   `session_mismatch`. A session older than `SESSION_MAX_AGE_SECONDS` (12 h) is
   ignored, so a stale file cannot select the wrong account.
5. `bot/proxy_bot.py::start` and `bot/bot.py::main` refuse to run when
   `session_mismatch()` reports a different account, exiting with code 2.
6. The detected session is the **default** account for every command, so no flag
   is needed. An explicit `--<username>` simply overrides it - and is still
   verified, so a wrong override is refused rather than silently running.
7. The listener treats a live login as authoritative for path binding, so a
   session's captures cannot be filed under whichever account the control file
   last mentioned.

Because the guard only fires when a session file exists, running without the
listener behaves exactly as before.

### `db populate` modes

`db populate` is one-shot by default: scan the account's capture folder, ingest
anything new, print a summary, exit. `--watch` keeps it running until Ctrl+C,
re-scanning every `--poll` seconds (default 10; note `--poll` is an *interval*,
not a run duration). Watch mode:

* only reads captures whose write has settled (`WATCH_ACTIVE_GRACE_SECONDS`), so
the file the listener is still appending to is skipped until it rolls;
* remembers `(size, mtime)` per file so an unchanged capture is not re-hashed
every pass (`processed_log_file` alone would catch it, but only after reading the
whole file);
* reports **every** pass, breaking the count into `with_data` / `no_map_data` /
`already_done`, plus `rbc_total`. `process_log_file` returns that status so a
legitimate zero cannot be mistaken for a failure;
* exits 0 on Ctrl+C with a summary rather than a traceback.

RBC data only appears when the game emits GAA map responses (opening the map, or
a scan run), so a run over idle captures correctly yields zero.

Together with the listener pruning already-ingested captures, the pipeline is
self-sustaining: listener writes, populate ingests, pruner deletes.

### Migrating keys

`python -m bot.cli db migrate-keys [--apply]` renames legacy numeric scope keys
onto login-name keys across every managed table and renames the data directory.
It refuses to merge two accounts into one key (both old and new having rows),
so data cannot silently collapse. Run it dry first.

### Why `db populate` has no logs-dir fallback

`db populate` defaults to the account's own `bot/account_data/<aid>/logs` and
will not fall back to the shared legacy `bot/logs`. Those captures belong to
whichever account produced them, so a fallback would silently inject one
account's RBC targets under a different aid — the exact cross-contamination the
aid-scoped model exists to prevent.

## Historical design notes → where they live now

The original working notes in `bot/bot.py` are now implemented:

| Note | Implemented as |
| --- | --- |
| Return time = travel duration + return duration, heuristic 20 min | `estimated_return_seconds`, `HEURISTIC_RETURN_SECONDS`, `RETURN_HEURISTIC_MULTIPLIER` |
| Cooldown increase `4.4 + 2 ** rand(1, 4.5)` | `cooldown_extra_seconds()` |
| Base increase 3 hours + landed time | `target_next_epoch()` (`landed_at + 3 * 3600 + cooldown_extra_seconds()`) |
| Strict 20 s + random 0–10 s between request intervals | `REQUEST_INTERVAL_RANGE`, `wait_for_request_slot()` |
| ADI before CRA, shuffled | `ADI_TO_CRA_DELAY_RANGE`, `wait_between_adi_and_cra()` |
| Level-61 only: 50 crossbowmen on the left flank | `bot/attacks.py::SANDS_LV61` |
| Persist `last_attacked` and use it to gate re-attacks | `rbc.last_attacked`, `reserve_target_for_task()` |
| Commanders persist between sessions | `commander_state` table keyed by `(aid, lord_id)` |

## How to add an attack

1. Compose it in `bot/attacks.py` using the in-house builders:

   ```python
   SANDS_LV50 = Attack(
       wave1=wave(
           left=side(units=[(Troop.CROSSBOWMAN, 50)]),
       )
   )
   ```

2. Reference it from `bot/tasks.py` in a `task_definition(...)`.
3. Browse troop/tool ids with `python -m bot.cli troops`.

## Toward concurrent workers

The current design already keeps the pieces separable for a future concurrent
message system:

* `bot/packets.py` is pure, so it is safe to import in any worker.
* Attack/task data (`bot/attacks.py`, `bot/tasks.py`) is plain data with no
  process affinity.
* All cross-process state goes through Postgres, keyed by `aid`, and the
  existing locking uses `FOR UPDATE SKIP LOCKED` — which is exactly what lets
  several workers share one queue safely.
* Per-account control files mean one process per account is already isolated.

When moving to parallel workers: give each worker its own `Account`, keep using
`SKIP LOCKED` for target/commander reservation, and replace the control-file
handshake in `bot/bot.py` with a message bus. Nothing in `packets`, `attacks`,
`tasks` or `scheduler` needs to change.
