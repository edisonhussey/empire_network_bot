# The lookup tables

`data/tables/` holds the game's data as small CSV files, **named with the game's own field names**, and
`crates/empire-game` mirrors them in Rust. This is the one copy the code and these docs agree on. The original files they
are built from are in `data/raw/` ([`README.md`](README.md)); the tables are the part that is committed.

## The pipeline

```text
data/raw/game_files/            the downloaded game files, untouched (git-ignored, large)
        |
        |  utils/scripts/build_tables.py     (re-run it; the output is deterministic)
        v
data/tables/*.csv + meta.json   the lookup tables: one place to look anything up (committed)
        |
        v
crates/empire-game/src/generated/{units,rules}.rs   the same rows as Rust statics
        |
        v
empire_game::unit_by_id, kingdom_rule, minute_skips   what the rest of the code calls
```

`data/tables/meta.json` records which items file and language file the tables came from (name, sha256, row counts), so a table can
always be traced back to the exact capture. Nothing in the Rust is hand-written data: `crates/empire-game/tests/unit_tables.rs`
fails if a generated row differs from its CSV row, so a hand edit or a stale rebuild cannot slip in.

## The tables

| File | Rows | Is |
| --- | ---: | --- |
| `units.csv` | 1001 | Every unit **and tool** the game defines (they are one table in the game): all 103 game columns plus three of ours |
| `kingdoms.csv` | 6 | Every kingdom: ids, internal names, display names, travel time, tax rates, skip costs, minimum levels |
| `minute_skips.csv` | 7 | The minute-skip items: `MST` key, currency id, minutes |
| `unit_art.csv` | 1001 | The picture of every unit: the client's name for its art, the versioned path, and whether its descriptor and image are in the raw copy ([Pictures](#pictures)) |
| `entity_art.csv` | 4076 | The same for buildings, horses and wishing wells |
| `captured_images.csv` | 237 | Every picture in the raw copy, with the entities it belongs to and a size check against its descriptor |
| `legacy/unit_names.csv` | 551 | Hand-kept bridge: `wodID` to the name the older Rust tables used. An **input**, not generated |
| `legacy/kingdom_names.csv` | 6 | The same for kingdoms |
| `meta.json` | | Where the tables came from |

Looking something up is a search, not a code read:

```sh
grep -i valkyrie data/tables/units.csv | cut -d, -f1-8        # find by name
grep -n "^214," data/tables/units.csv                          # find by id
column -s, -t < data/tables/kingdoms.csv | cut -c1-120         # the kingdoms
```

## Naming: one rule so a field is findable everywhere

* **The game's own names are kept as they are.** CSV columns are the items file's fields (`wodID`, `kingdomTravellingCost`,
  `unitTravelTaxRate`); packet fields keep their wire names (`SCID`, `KID`, `MST`) in the packet docs and in any table built from
  packets.
* **Rust fields are those names in snake_case**, nothing renamed: `kingdomTravellingCost` is `kingdom_travelling_cost`,
  `unitTravelTaxRate` is `unit_travel_tax_rate`, `meleeDefence` is `melee_defence`, `kID` is `k_id`. `type` is `type_name` only because
  `type` is a Rust keyword. (The older tables said `ranged_defence` and `loot_capacity`; the game says `rangeDefence` and `lootValue`.)
* **Columns we add are lowercase snake_case** (`display_name`, `legacy_name`, `legacy_kind`) so they are never mistaken for a game field.
* **Human-readable names are columns, not renames.** `display_name` is what the player reads; `legacy_name` is what this project's older
  code called it. Search either to find the id, then the id to find the row.

## The `units.csv` columns

Documented (meaning established from the client code, a capture or the old tables):

| Column | Meaning |
| --- | --- |
| `wodID` | The unit id every packet uses (`AC` in `dcl`, `A` in `kut`/`cds`/`cra`). |
| `type` | The game's internal name (`MeadRanger`); the key of the language entry `<type>_name`. |
| `display_name` | **Added by us.** The player-facing name from the language file (`<type>_name`); empty for a sixth of the rows. |
| `legacy_name` | **Added by us.** The name the older Rust tables used (`valkyrie_ranger_9`); empty for units they did not have. |
| `legacy_kind` | **Added by us.** `troop` or `tool` as the older tables classified it. |
| `role` | `melee` or `ranged` for a troop; **empty for a tool**. The game's own troop-or-tool test. |
| `comment1` | Where it comes from ("Relic Barracks", "Nomad Shop"). |
| `comment2` | What it is ("Ranged Attacker", "Crossbowman"). |
| `name` | The building or source it belongs to (`Barracks`, `Workshop`, `Eventunit`); not the unit's display name. |
| `group` | Always `Unit` in this file (tools are in the same table). |
| `downgradeWodID` | The unit below this one. |
| `eventIDs` | Events the unit belongs to. |
| `foodSupply` | Food it consumes. |
| `kingdomTravellingCost` | Coins to send one to another kingdom, before the target kingdom's tax ([`../game_infrastructure/kingdom_transfer_rules.md`](../game_infrastructure/kingdom_transfer_rules.md)). Empty if it cannot be sent. |
| `level` | The unit's level within its line. |
| `lootValue` | Loot it carries (the old tables call this `loot_capacity`). |
| `meleeAttack` | Melee attack. |
| `meleeDefence` | Defence against melee. |
| `mightValue` | Might points. |
| `rangeAttack` | Ranged attack. |
| `rangeDefence` | Defence against ranged. |
| `speed` | Travel speed. |
| `upgradeWodID` | The unit this one upgrades into. |

Not yet documented (80 columns): `allowedToAttack`, `allowedToDefend`, `allowedToTravel`, `amountPerWave`, `beefSupply`, `buildingLevel`, `c1Bonus`, `canBeUsedByNPC`, `canBeUsedToAttackNPC`, `cleavageOfCellsCost`, `clientUsageEventID`, `costC1`, `costC2`, `costComponent1`, `costComponent2`, `costComponent3`, `costComponent4`, `costComponent5`, `costComponent6`, `costComponent7`, `costComponent8`, `costDragonGlassArrows`, `costDragonScaleArmor`, `costDragonScaleArrows`, `costLegendaryToken`, `costSceatToken`, `costStone`, `costTwinFlameAxes`, `costWood`, `defMeleeBonus`, `defRangeBonus`, `deleteToolAfterBattle`, `dollWod`, `effects`, `extraBossHealthUnit`, `factionID`, `fameAsDef`, `fameAsOff`, `fameBonus`, `fightType`, `fireBoost`, `gateBonus`, `healingCostC1`, `healingCostC2`, `healingOrder`, `healingTime`, `hybrid`, `isAuxiliary`, `isDormant`, `isKamikaze`, `isYardTool`, `kIDs`, `khanMedalBooster`, `khanTabletBooster`, `lowLevelRecruitmentTime`, `meadSupply`, `moatBonus`, `numberofDolls`, `offMeleeBonus`, `offRangeBonus`, `palaceLockedByKID`, `pearlBooster`, `pointBonus`, `ragePointBonus`, `recruitmentTime`, `reputationBonus`, `researchLocked`, `reviveAllCostC2`, `samuraiTokenBooster`, `skillUnlockID`, `skipCost`, `skipCostC2`, `slotTypes`, `sortOrder`, `tempServerCostC2`, `toolCategory`, `typ`, `usageEventID`, `wallBonus`, `xpBonus`.
Their meaning is not written down yet; add a row above the first time something depends on one. Do not guess a meaning here.

## Pictures

The raw copy now has pictures: **237 images**, captured by `proxy_packet_monitor` with "Include images" (232 item pictures under `assets/itemassets/`,
5 interface sheets under `assets/interface/`), each beside its sprite descriptor (`.json`, CreateJS sheet metadata). They are `.webp` files: the
descriptors still say `.png`, but the client requests `.webp` and the capture saved that.

**How a picture is tied to a game entity** (every link is checked, see below):

1. **Picture to name.** The client's `ggs.dll` has a table from art class name to versioned path (8,143 entries,
   `Workshop_Unit_Ram="itemassets/Units/Workshop/Workshop_Unit_Ram/Workshop_Unit_Ram--1573584429307"`). All 232 captured item pictures are found in it by path.
2. **Name to entity.** For things defined as rows in the items file, the client names the art `<name>_<group>_<type>` (`AVisualVO.getVisualClassName`):
   unit 214 is `Barracks_Unit_MeadRanger`, building 800 is `Bakery_Building_Level5`. That rule covers four sections: `units` (1,001 rows), `buildings` (4,032), `horses` (33) and `wishingwells` (11).
3. **Address.** `https://empire-html5.goodgamestudios.com/default/assets/<item_asset_path>.webp` (no column holds it; it is this rule).

Several entities can share one picture (levels of a line): 1,001 units use 501 pictures, and the 70 captured pictures tied to buildings cover 810 building rows.

| Table | Rows | Is |
| --- | ---: | --- |
| `unit_art.csv` | 1001 | Every unit: its `visual_class_name`, `item_asset_path`, version, and whether its descriptor and image are in the raw copy |
| `entity_art.csv` | 4076 | The same for buildings, horses and wishing wells (`section` + `wodID` identify a row; `wodID` alone is not unique across sections) |
| `captured_images.csv` | 237 | **Every picture in the copy**: path, class, the entities it belongs to (`section:wodID`), its real size, the size its descriptor declares, and whether they match |

**Verified:** for all 237 captured pictures the real width and height equal the sheet size the descriptor declares (`size_matches_descriptor` is `yes` throughout), which ties picture, descriptor and
class together; and `unit_tables.rs` fails if that ever stops being true. **Entities tied:** 70 of the 237 (69 buildings and one surroundings piece).

**Not tied to an entity, honestly:** the other 167 pictures are status icons (66), world-map pieces (46), interface sheets (5), dialogs, backgrounds, FX, crests and so on. They are art the client
picks by its own code (a status, a map object), not rows in the items file, so there is no table to join them to; they are listed in `captured_images.csv` with their class name and category only.
**No unit picture has been captured yet** (`image_captured` is `no` for all 1,001): unit art loads when units are shown, so it needs a session that opens the barracks, the army or the troop pickers
with "Download login data" and "Include images" on. The unit addresses are predicted from the rule above; none has been fetched.

**To get more pictures:** tick both boxes in `proxy_packet_monitor`, start the proxy, hard-reload the game, and show what you want captured; then
`python3 utils/scripts/organize_game_data.py` (copies them into `data/raw/game_files/assets/`, giving every captured URL its own path even when two pictures are byte-identical) and
`python3 utils/scripts/build_tables.py`.

The same table also covers 7,100 other pictures (equipment 1,082, world map 770, collectables, icons, ...) not extracted into per-entity tables yet; `read_art_table` in `build_tables.py` reads it.

## What still uses the older tables, and why

The older `troops`, `tools` and `kingdoms` tables (`generated/troops.rs` and friends, 257 troops and 294 tools) still decide **which units the bot
sends**: the attack builder's catalogue, the support army, and the support fallback (`known_troops_only`). They were left alone on purpose:
the complete table knows hundreds of units the bot has never sent, including big stacks in a real account (215 "Shield-maiden",
227 "Protector of the north"), and making those eligible would change live behaviour without anyone deciding to.

What the complete table now drives instead:

* **Names on screen:** the Castle Overview and the Troop overview show the game's names and its troop/tool split (`station::unit_label`), so 214 reads "Valkyrie ranger" and 215 "Shield-maiden" instead of `unit 215`.
* **Kingdom transfer prices, journey times, tax rates and skip items** ([`../architecture/kingdom-transfer.md`](../architecture/kingdom-transfer.md)).

Moving the rest (attack and support eligibility) onto the complete table is a behaviour decision, not a data one: it would be done by adding an
explicit column for "the bot may send this" and reviewing it. Until then the older tables are the allow-list.

## Rebuilding

```sh
python3 utils/scripts/organize_game_data.py           # sort a new capture into data/raw/game_files
python3 utils/scripts/build_tables.py                 # tables + generated Rust (add --no-rust for tables only)
cargo test -p empire-game                             # the CSV and the Rust must agree
```

After a game update only those three steps are needed; the diff of `data/tables/` is the readable list of what the game changed.

## Where this is going: captures in, tables out

The same shape is meant for packets: `proxy_packet_monitor` recordings go into `data/raw/captures/` as they are, a processing step turns
them into tables and the capture files in [`../network/captures/`](../network/captures), and the packet docs ([`../network/`](../network/README.md)) cite
those. Because every layer uses the wire field names (`SCID`, `KID`, `MST`, `UT`), a field seen in a recording, a column in a table and a
field in the Rust are the same word, so tracing one to the others needs no translation. Nothing of the packet side exists yet beyond the
captured files and the docs.
