# Build the project

The one command, from any directory (the `oa` alias in `~/.zshrc` points at `utils/scripts/oa`):

```zsh
oa build              # build the DMG for the version in VERSION, collect it, install it
oa build --no-build   # collect and install a DMG that is already built (does not rebuild)
```

Neighbours: `oa bump` (0.1.68 to 0.1.69; `oa bump 0.2.0` sets one), `oa version`, `oa test` (cargo test plus the
frontend build), `oa check` (fast `cargo check`), `oa clean` ([`clear_cache.md`](clear_cache.md)).

## What `oa build` does (`utils/scripts/release.sh`)

1. Makes every version file agree with `VERSION` (`sync-version.sh`) and refuses to continue if any still disagrees.
2. **Cleans** the desktop build (`cargo clean` on `desktop/src-tauri`), runs `npm ci`, then `npm run tauri build`, with the
   builder's home directory removed from the binary's panic paths.
3. Refuses to ship an app bundle that contains a database or token, and warns if the source is newer than the binary.
4. Copies `OpenAuto_<version>_aarch64.dmg` into `releases/`.
5. **Quits and kills the running OpenAuto app** and replaces `/Applications/OpenAuto.app` with the new one.

## Read before running

* Step 5 ends the running app and its service, which drops the live game session; reconnect from Start afterwards.
  If a bot must keep running, do not run `oa build`; run `oa test` or `oa check` instead (they touch nothing installed).
* Step 2 deletes the previous desktop build, including an old DMG still sitting in `desktop/src-tauri/target/release/bundle/`.
  Copy it to `releases/` first if you want it.
* It takes several minutes and several GB of disk. If the disk is nearly full, run `oa clean` first.
* The new window refuses an old running service whose `API_VERSION` differs (`crates/empire-daemon/src/lib.rs`); restart the app after installing.

Everything lives in `utils/scripts/`; see [`README.md`](README.md).
