# utils

Everyday commands for building, cleaning and resetting the project, and the scripts behind them.

| I want to | Run | Read |
| --- | --- | --- |
| Build and install the app | `oa build` | [`build.md`](build.md) |
| Free the debug build cache | `oa clean` | [`clear_cache.md`](clear_cache.md) |
| Reset to a first-run install | `utils/scripts/fresh-start.sh --dry-run`, then without the flag | [`wipe_storage.md`](wipe_storage.md) |
| Run the tests / a fast check | `oa test` / `oa check` | |
| Change the version | `oa bump` / `oa bump 0.2.0` | [`build.md`](build.md) |
| Sort a new game-data capture | `python3 utils/scripts/organize_game_data.py` | [`../docs/game_data/README.md`](../docs/game_data/README.md) |
| Rebuild the lookup tables (and the Rust that mirrors them) from the raw game files | `python3 utils/scripts/build_tables.py` | [`../docs/game_data/tables.md`](../docs/game_data/tables.md) |

`oa` is `utils/scripts/oa`; the alias lives in `~/.zshrc` (`alias oa='…/rust_distributable/utils/scripts/oa'`). It works through a symlink too.

## The scripts (`utils/scripts/`)

| File | Does | Touches the running app? |
| --- | --- | --- |
| `oa` | The entry point: build, bump, version, test, check, clean | only `oa build` |
| `release.sh` | Clean build, DMG into `releases/`, install | **yes: quits and replaces it** |
| `sync-version.sh` | Copies `VERSION` into `desktop/package.json`, `package-lock.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` | no |
| `fresh-start.sh` | Backs up and removes the database | **yes: stops it** |
| `organize_game_data.py` | Copies and sorts a login-data capture into `data/raw/game_files/` | no |
| `build_tables.py` | Raw game files to `data/tables/*.csv` and `crates/empire-game/src/generated/{units,rules}.rs` | no |

Every script finds the project from its own location (two levels up), so they run from any directory.

## Retired: `gen_game_data.py`

It generated the troop, tool and kingdom tables from the old Python bot's data. Those tables are now maintained by hand in
`crates/empire-game/src/generated/` and the script is archived in `useless/`. The game's own items file
(`data/raw/game_files/rules/items/`, now the source of `data/tables/units.csv`) agrees with the Rust troop table on speed for all 257 troops but differs on defence or loot for about
10% of them, so the tables lag the live game version. If they should follow it, write a generator from the items file
(`build_tables.py` is the model for it) and review the diff first.
