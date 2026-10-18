# Clearing the build cache safely

The command is `oa clean` ([`../../utils/clear_cache.md`](../../utils/clear_cache.md)). This page says what it removes and
why that is safe.

## What the cache is

Rust builds into a `target/` folder next to each Cargo project. It holds compiled copies of your code and of every
dependency; nothing in it is hand-written, and all of it is rebuilt from source on demand.

| Folder | What | Typical size | Safe to delete |
| --- | --- | ---: | --- |
| `rust_distributable/target/debug` | Test and check builds (`cargo test`, `cargo check`), incremental caches | about 9 GB | Yes. The next test run rebuilds it in minutes |
| `rust_distributable/desktop/src-tauri/target/debug` | The desktop shell's debug build | about 1 GB | Yes |
| `rust_distributable/target/release` | Release builds | about 0.4 GB | Yes, but a release then rebuilds from scratch |
| `rust_distributable/desktop/src-tauri/target/release` | The release build **and the built DMG/app bundle** (`bundle/dmg/`, `bundle/macos/`) | about 1 GB | Only after copying a DMG you want to keep to `releases/` |
| `desktop/node_modules`, `desktop/dist` | Front-end dependencies and output | small | Yes (`npm ci` restores) |

`oa clean` deletes only the two `debug` folders.

## Why it cannot touch source

* `cargo clean` removes the `target/` directory of the project it is run in, and nothing else. It never reads or deletes `src/`.
* All these folders are in `.gitignore` (`/target/`, `/desktop/src-tauri/target/`, `/desktop/dist/`, `/desktop/node_modules/`), so
  none of them holds a tracked file. Check any time with `git ls-files target desktop/src-tauri/target | wc -l`, which must print 0.
* The installed app (`/Applications/OpenAuto.app`), the database (`~/Library/Application Support/com.openauto.desktop/`),
  `releases/` and `data/` are all outside what it removes.

## When to do it

When the disk is tight (a debug build alone can fill several GB), or before a big dependency change. It makes the next build
slower, never wrong. Free space check: `df -h /System/Volumes/Data`.

## What is not a build cache

`data/raw/game_files/` (the game's files), the proxy monitor's `web_data/` and `data/sessions/` (recordings) and the database are
**data**, not cache. They are not removed by `oa clean`; see [`../../utils/wipe_storage.md`](../../utils/wipe_storage.md) for the
database and [`../game_data/README.md`](../game_data/README.md) for the game files.
