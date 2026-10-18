# data/

* `raw/` the originals: `game_files/` (the downloaded game files, sorted by function) and, later, `captures/` (proxy recordings). **Git-ignored**, large.
* `tables/` the lookup tables built from `raw/` by `utils/scripts/build_tables.py` (units, kingdoms, skip items, plus the small hand-kept `legacy/` name bridges). **Committed**.
* `retired/` old databases from the first Rust builds; nothing reads them. Git-ignored.

Manual: `docs/game_data/README.md` (the raw files) and `docs/game_data/tables.md` (the tables, the naming rule and the pipeline).
