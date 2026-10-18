# Game data: the manual

The game's data lives in two places under `data/`:

* **`data/raw/game_files/`** (this page): a copy of the files the game client downloads at login (about 46 MB, 493 files), sorted by what each
  does. These are the originals and are **local only** (`data/raw/` is git-ignored).
* **`data/tables/`** ([`tables.md`](tables.md)): the small lookup tables extracted from them (units, kingdoms, skip items, by the game's own
  field names). These are **committed**, and `crates/empire-game` mirrors them in Rust.

This page is the manual for the raw files; the notes in this folder are committed.

Captured 2026-10-14 with `proxy_packet_monitor` ("Download login data"), client build with `Game.bundle.4230c06b...`,
items version **787.03**, language bundle **4375**.

## Layout

| Folder | Holds | Read |
| --- | --- | --- |
| [`client/`](../../data/raw/game_files/client) | The game's program: the bundles, `ggs.dll`, `index.html`, styles | [`client.md`](client.md) |
| [`rules/items/`](../../data/raw/game_files/rules/items) | `items_v787.03.json` (19 MB), the game's rule tables, and its version file | [`items.md`](items.md) |
| [`languages/`](../../data/raw/game_files/languages) | `texts/4375/en.json` (30,258 text keys), `texts/version.json` (the version of every language), `cldr/en-GB.json` (number and date formats), `langserv/` (live text snippets) | below |
| [`config/`](../../data/raw/game_files/config) | `network/1.xml` (the server list: host, port, zone), `country/country.xml` (countries, language, time zone), `branches/branches.xml` (client branches) | below |
| [`assets/`](../../data/raw/game_files/assets) | Art **descriptors** (not images): `interface/` (5 groups: start screen, interface elements, icons, shared elements, fonts) and `itemassets/` (462 JSON files by category: buildings, units, world map, status icons, ...) | below |
| [`unsorted/`](../../data/raw/game_files/unsorted) | Two near-empty remote-config files whose purpose is not confirmed (`eyecatcher-assets/live.json` is `{"current":""}`, `status/12/maint.json` is `[]`) | |
| `manifest.jsonl` | One line per file: where it was put, where it came from, URL, type, size, sha256 | |

### Languages
* `texts/4375/en.json` is a flat object of text keys to strings (`dialog_..._...`), 30,258 of them. The Traveling Knights are `eventknight_name`. A key's text is what the player reads; use it to turn an id from the rules into a name.
* `texts/version.json` maps each language code to its bundle version (`"en": "4375"`); the language server (`langserv`) carries newer one-key snippets (version 4376 at capture).

### Config
* `network/1.xml` is the world list the client connects to: for instance `ep-live-mz-int1-sk1-gb1-game.goodgamestudios.com`, zone `EmpireEx`. The server header the bot sends (`EmpireEx_21`, `EmpireEx_49`) is the zone plus the instance.

### Pictures
* The copy has the pictures captured so far (`.webp`, 237 files) beside their sprite descriptors; the map from a picture to the entity it shows is in `data/tables/` (`captured_images.csv`, `unit_art.csv`, `entity_art.csv`), explained in [`tables.md`](tables.md#pictures).

### Assets
* An item-asset descriptor says which image or animation a building, unit or icon uses. They are useful for finding what an item looks like or is called in the client, not for rules. The rules are in the items file.

## How it is produced and refreshed

1. In `proxy_packet_monitor`, tick **Download login data** (and **Include images** for the pictures), start the proxy, hard-reload the game and log in. The capture lands in `proxy_packet_monitor/web_data/login_data/` (see its README).
2. `python3 utils/scripts/organize_game_data.py` copies and sorts it into this folder (add `--dry-run` to preview). Re-running skips files already there. A file it cannot place goes to `unsorted/` under its host and path.
3. `python3 utils/scripts/build_tables.py` builds the lookup tables in `data/tables/` and the Rust that mirrors them ([`tables.md`](tables.md), [`../../utils/README.md`](../../utils/README.md)).

## What depends on it

* The lookup tables `data/tables/*.csv` and the generated Rust (`crates/empire-game/src/generated/units.rs`, `rules.rs`): every unit and its name, kingdom rules, skip items ([`tables.md`](tables.md)).
* The robber baron level formula and the packet notes read from the client code ([`client.md`](client.md), [`../network/`](../network/README.md)).
* Nothing reads this folder at run time; the app ships without it.
