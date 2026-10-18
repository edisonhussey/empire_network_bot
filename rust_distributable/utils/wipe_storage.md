# Wipe local storage (simulate a fresh install)

```zsh
cd ~/Desktop/github_gge_bot/empire_network_bot/rust_distributable
utils/scripts/fresh-start.sh --dry-run    # shows what it would do, changes nothing
utils/scripts/fresh-start.sh              # does it
KEEP_BACKUPS=5 utils/scripts/fresh-start.sh
```

It returns OpenAuto to a first-run state so it can be tested as a new user receives it: with no stored licence the app
stops at the activation screen and the next token is bound to the account that initialises it.

## What it does

1. Finds the data folder (macOS: `~/Library/Application Support/com.openauto.desktop`, the same one `empire-core/src/paths.rs` uses).
2. **Stops OpenAuto** (`pkill`), because the running service owns the database. This ends any live session and any running bot.
3. Copies `empire.sqlite3` to `empire.sqlite3.before-fresh-start-<timestamp>` (the database is never destroyed), removes the live
   database and its `-wal` / `-shm` files, and keeps only the newest `KEEP_BACKUPS` backups (default 2).

## Read before running

* Always run `--dry-run` first. It lists every file it would remove.
* It does not touch source code, `releases/`, `data/raw/game_files/`, or the proxy monitor's recordings.
* To get the old data back, stop the app and copy a backup over `empire.sqlite3`.
* `EMPIRE_DATA_DIR`, if set, is used by the app but **not** by this script, which always works on the platform folder.
