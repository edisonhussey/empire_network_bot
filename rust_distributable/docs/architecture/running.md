# Running and verifying

How to run each piece, where the data goes, and how to check a change. (Carried over from the original README; the
data-folder rule is corrected to what `empire-core/src/paths.rs` does.) Everyday commands are in [`../../utils/README.md`](../../utils/README.md).

## Where the data lives

One answer, asked of `empire_core::paths::data_dir()` by every process (desktop, embedded service, `empire-hunt`, the
stand-alone daemon), so two processes never open one database by different routes:

1. `EMPIRE_DATA_DIR`, when set and non-empty.
2. The platform application folder: macOS `~/Library/Application Support/com.openauto.desktop/`.
3. `./data`, only if the platform gives no home directory.

The database is `empire.sqlite3` in that folder. (`rust_distributable/data/` is **not** a data folder: `data/retired/` holds old
databases and `data/raw/game_files/` the downloaded game files, see [`../game_data/README.md`](../game_data/README.md).)

## The desktop app

```sh
cd rust_distributable/desktop
npm install
npm run tauri dev
```

It starts the daemon itself and keeps its data in the platform folder above. Do not run the stand-alone daemon on the same
port at the same time.

On start the window compares the running service's `api_version` (`GET /v1/health`) with its own and replaces a service that is older, whether that is the
app's own `--service` process or a stand-alone `empire-daemon` ([problem 19](../problems/19.md)). A stale stand-alone daemon (for example one started by a process monitor from
an old `target/release` build) otherwise answers every newer route with `404` and fails on the migrated database. Rebuild it after any change: `cargo build --release -p empire-daemon`.

## The stand-alone daemon (network tests without the window)

```sh
cd rust_distributable
cargo run -p empire-daemon
```

Transport check against the live game, no credentials (expects `HTTP 101`, `permessage-deflate` and `apiOK`): `cargo run -p empire-socket --example verchk`.

* API: `127.0.0.1:47821` (override `EMPIRE_BIND`), logging `RUST_LOG`, data folder `EMPIRE_DATA_DIR`.
* Useful endpoints:

```text
GET  /v1/health
GET  /v1/messages?limit=50
POST /v1/injections       {"packet":"%xt%gbl%1%{}%","ttl_ms":15000}
GET  /v1/direct           current direct-session status
POST /v1/direct           open a direct session
DELETE /v1/direct         close it
GET  /v1/relay?upstream=wss://example.invalid/socket   (WebSocket upgrade)
```

Injected packets are XT-validated, time-limited and pass through a bounded 64-item queue ([`decisions.md`](decisions.md)).

## The login proof (a standalone binary with no daemon, database or window)

```sh
cd rust_distributable
cargo run --release -p empire-login -- --config /path/to/account.ini
```

or, environment only:

```sh
GGE_USERNAME='account-name' GGE_ACCOUNT_ID='portal-account-id' GGE_PASSWORD='account-password' \
cargo run --release -p empire-login
```

`GGE_LOGIN_TOKEN` may replace `GGE_PASSWORD`; other overrides: `GGE_SERVER_URL`, `GGE_SERVER_HEADER`, `GGE_RCT`,
`GGE_CLIENT_VERSION`, `GGE_CONM`, `GGE_RTM`, `GGE_LOGIN_TIMEOUT`. It makes one login attempt, never prints credentials or the login
packet, closes after the proof and does not retry. A good run ends with lines like:

```text
LOGIN_PROOF command=lli status=0 authenticated=true
SANDS_MAP_PROOF kingdom_id=1 tiles_loaded=6
SANDS_CASTLE_PROOF kingdom_id=1 castle_id=... name="..." coordinates=...:...
FULL_PROOF authenticated=true main_castle=true sands_map=true sands_castle=true inventory=true
```

The frame order it follows: [`../network/sequences/login.md`](../network/sequences/login.md).

## Issuing a test application token

Administrator-only. The private key is git-ignored and must never be copied into a customer build.

```sh
cd rust_distributable
cargo run -q -p empire-license-admin -- issue --private admin/master_key.txt \
  --license-id local-four-day-test --subject developer --server US1 \
  --bootstrap-x 509 --bootstrap-y 405 --days 4 --revision 1
```

Paste the token into the first-launch screen. A renewal uses the same licence id and a higher revision. Key handling and
rotation: [`../../admin/licence_admin.md`](../../admin/licence_admin.md).

## Verifying a change

```sh
oa check        # cargo check --workspace --tests
oa test         # cargo test --workspace, then the front-end build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cd desktop && node --test src/        # front-end unit tests
```
