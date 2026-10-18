#!/usr/bin/env python3
"""Build the lookup tables (and the Rust that mirrors them) from the raw game files.

    raw game files                      hand-kept bridges
    data/raw/game_files/...             data/tables/legacy/*.csv
              \\                          /
               utils/scripts/build_tables.py
              /                          \\
    data/tables/*.csv (committed)     crates/empire-game/src/generated/{units,rules}.rs
      (units, kingdoms, minute_skips, unit_art)

* **The CSV columns are the game's own field names**, exactly as in the items file (`wodID`, `kingdomTravellingCost`,
  `unitTravelTaxRate`, ...). The few columns we add are `snake_case` (`display_name`, `legacy_name`, `legacy_kind`) so the two are
  never confused.
* **The Rust fields are the same names in snake_case** (`wod_id`, `kingdom_travelling_cost`, `unit_travel_tax_rate`); `type`
  becomes `type_name` only because `type` is a Rust keyword.
* Everything is deterministic (sorted, no timestamps), so re-running on the same raw files changes nothing.

    python3 utils/scripts/build_tables.py              # tables and Rust
    python3 utils/scripts/build_tables.py --no-rust    # tables only
    python3 utils/scripts/build_tables.py SRC          # SRC = another raw game_files folder

The manual is ``docs/game_data/tables.md``.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import struct
import sys
from pathlib import Path

RUST_DIR = Path(__file__).resolve().parents[2]
# Where the client fetches item art from: the captured descriptors were served from `<host>/default/assets/<path>.json`.
ASSET_BASE = "https://empire-html5.goodgamestudios.com/default/assets"
# A picture's address is `ASSET_BASE/<item_asset_path>` + IMAGE_EXT (no column holds it: it is derivable).
# The pictures are requested as `.webp` (the capture saved them under that name), although the sprite descriptors still say `.png`.
IMAGE_EXT = ".webp"
# Items sections whose rows the client names art for as `<name>_<group>_<type>` (AVisualVO.getVisualClassName).
ART_SECTIONS = ("units", "buildings", "horses", "wishingwells")
RAW = RUST_DIR / "data" / "raw" / "game_files"
TABLES = RUST_DIR / "data" / "tables"
GENERATED = RUST_DIR / "crates" / "empire-game" / "src" / "generated"

# Columns that come first in units.csv; every other game field follows, sorted.
UNIT_LEAD = ["wodID", "type", "display_name", "legacy_name", "legacy_kind", "role", "comment1", "comment2", "name", "group"]
KINGDOM_LEAD = ["kID", "kingdomName", "display_name", "legacy_name", "legacy_constant"]

# The unit fields the Rust `Unit` carries: (rust field, game field). The rest are in units.csv.
UNIT_NUMBERS = [
    ("level", "level"), ("speed", "speed"), ("melee_attack", "meleeAttack"), ("range_attack", "rangeAttack"),
    ("melee_defence", "meleeDefence"), ("range_defence", "rangeDefence"), ("loot_value", "lootValue"),
    ("might_value", "mightValue"), ("food_supply", "foodSupply"),
    ("kingdom_travelling_cost", "kingdomTravellingCost"),
    ("upgrade_wod_id", "upgradeWodID"), ("downgrade_wod_id", "downgradeWodID"),
]
KINGDOM_NUMBERS = [
    ("unit_travel_time", "unitTravelTime"), ("resource_travel_time", "resourceTravelTime"),
    ("unit_travel_tax_rate", "unitTravelTaxRate"), ("resource_travel_tax_rate", "resourceTravelTaxRate"),
    ("min_level", "minLevel"), ("skip_unit_travel_c2_cost", "skipUnitTravelC2Cost"),
    ("skip_resource_travel_c2_cost", "skipResourceTravelC2Cost"),
]


def read_csv(path: Path) -> list[dict]:
    if not path.exists():
        return []
    with path.open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def write_csv(path: Path, lead: list[str], rows: list[dict]) -> None:
    columns = lead + sorted({key for row in rows for key in row} - set(lead))
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns, lineterminator="\n", extrasaction="ignore")
        writer.writeheader()
        for row in rows:
            writer.writerow({column: row.get(column, "") for column in columns})


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def rs_str(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'


def rs_opt_str(value: str) -> str:
    return f"Some({rs_str(value)})" if value else "None"


def rs_opt_int(value: str) -> str:
    try:
        return f"Some({int(float(value))})" if str(value).strip() != "" else "None"
    except ValueError:
        return "None"


ART_ENTRY = re.compile(r'([A-Za-z0-9_]+)="(itemassets/[^"]+)"')


def read_art_table(raw: Path) -> tuple[dict[str, str], Path | None]:
    """The client's table of item-art class names to versioned paths (`ggs.dll`: `Workshop_Unit_Ram="itemassets/Units/..."`)."""
    dll = next((raw / "client" / "dll").glob("ggs.dll.*.js"), None)
    if not dll:
        return {}, None
    return {m.group(1): m.group(2) for m in ART_ENTRY.finditer(dll.read_text(encoding="utf-8"))}, dll


def image_size(path: Path) -> tuple[int, int] | None:
    """Width and height from the file header: webp (lossy, lossless, extended) or png."""
    head = path.read_bytes()[:40]
    if head[:4] == b"RIFF" and head[8:12] == b"WEBP":
        kind = head[12:16]
        if kind == b"VP8 ":
            w, h = struct.unpack("<HH", head[26:30])
            return w & 0x3FFF, h & 0x3FFF
        if kind == b"VP8L":
            bits = int.from_bytes(head[21:25], "little")
            return (bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1
        if kind == b"VP8X":
            return 1 + int.from_bytes(head[24:27], "little"), 1 + int.from_bytes(head[27:30], "little")
    if head[:8] == b"\x89PNG\r\n\x1a\n":
        return struct.unpack(">II", head[16:24])
    return None


def descriptor_size(path: Path) -> tuple[int, int] | None:
    """The sheet size a sprite descriptor declares (`size: {w, h}`)."""
    try:
        size = json.loads(path.read_text(encoding="utf-8")).get("size") or {}
        return int(size["w"]), int(size["h"])
    except (OSError, ValueError, KeyError, TypeError):
        return None


def build(raw: Path, rust: bool) -> None:
    items_path = next((raw / "rules" / "items").glob("items_v*.json"), None)
    lang_path = next((raw / "languages" / "texts").glob("*/en.json"), None)
    if not items_path or not lang_path:
        sys.exit(f"no items file or English texts under {raw}; run utils/scripts/organize_game_data.py first")
    items = json.loads(items_path.read_text(encoding="utf-8"))
    lang = json.loads(lang_path.read_text(encoding="utf-8"))
    version = items.get("versionInfo", {}).get("version", {}).get("@value", "unknown")

    legacy_units = {int(r["wodID"]): r for r in read_csv(TABLES / "legacy" / "unit_names.csv")}
    legacy_kingdoms = {int(r["kID"]): r for r in read_csv(TABLES / "legacy" / "kingdom_names.csv")}

    def label(kind_name: str) -> str:
        """The player-facing name: the language key `<type>_name`, either capitalisation."""
        for key in (f"{kind_name}_name", f"{kind_name.lower()}_name"):
            if isinstance(lang.get(key), str):
                return lang[key]
        return ""

    units = []
    for unit in sorted(items["units"], key=lambda u: int(u["wodID"])):
        row = {key: str(value) for key, value in unit.items()}
        legacy = legacy_units.get(int(unit["wodID"]), {})
        row["display_name"] = label(unit.get("type", ""))
        row["legacy_name"] = legacy.get("legacy_name", "")
        row["legacy_kind"] = legacy.get("legacy_kind", "")
        units.append(row)

    kingdoms = []
    for kingdom in sorted(items["kingdoms"], key=lambda k: int(k["kID"])):
        row = {key: str(value) for key, value in kingdom.items()}
        legacy = legacy_kingdoms.get(int(kingdom["kID"]), {})
        text = lang.get(f"kingdomName_{kingdom.get('kingdomName', '')}", "")
        row["display_name"] = text if isinstance(text, str) else ""
        row["legacy_name"] = legacy.get("legacy_name", "")
        row["legacy_constant"] = legacy.get("legacy_constant", "")
        kingdoms.append(row)

    skips = [
        {
            "MST": f"MS{int(row['MinuteSkipIndex']) + 1}",
            "currencyID": row["currencyID"],
            "MinuteSkipIndex": row["MinuteSkipIndex"],
            "MinutesSkipValue": row["MinutesSkipValue"],
        }
        for row in sorted(items["currencyMinutesSkipValues"], key=lambda r: int(r["MinuteSkipIndex"]))
    ]

    # The art of every unit: the client names it `<name>_<group>_<type>` (`AVisualVO.getVisualClassName`) and looks that name up in
    # the table above for the versioned path. Pictures were not downloaded; `descriptor_captured` says whether the sprite
    # descriptor (`.json`) is in the raw copy.
    art, dll = read_art_table(raw)
    have = {p.relative_to(raw / "assets").as_posix() for p in (raw / "assets").rglob("*") if p.suffix in (".json", ".webp", ".png")}
    unit_art = []
    for u in units:
        visual = f"{u.get('name', '')}_{u.get('group', 'Unit')}" + (f"_{u['type']}" if u.get("type") else "")
        path = art.get(visual, "")
        art_version = path.rsplit("--", 1)[-1] if "--" in path else ""
        unit_art.append({
            "wodID": u["wodID"], "type": u.get("type", ""), "name": u.get("name", ""), "display_name": u["display_name"],
            "visual_class_name": visual, "item_asset_path": path, "item_asset_version": art_version,
            "descriptor_captured": "yes" if path and f"{path}.json" in have else "no" if path else "",
            "image_captured": "yes" if path and (f"{path}.webp" in have or f"{path}.png" in have) else "no" if path else "",
        })

    # Every other entity the client names art for the same way (buildings, horses, wishing wells), and every captured picture with the
    # entities it belongs to. Class names are not unique per section, so an entity is `<section>:<wodID>`.
    classes: dict[str, list[str]] = {}
    entity_rows = []
    for section in ART_SECTIONS:
        for row in items.get(section, []):
            if not (isinstance(row, dict) and row.get("name") and row.get("group") and "wodID" in row):
                continue
            visual = f"{row['name']}_{row['group']}" + (f"_{row['type']}" if row.get("type") else "")
            classes.setdefault(visual, []).append(f"{section}:{row['wodID']}")
            if section == "units":
                continue
            path = art.get(visual, "")
            entity_rows.append({
                "section": section, "wodID": str(row["wodID"]), "name": row["name"], "type": row.get("type", ""),
                "level": row.get("level", ""), "visual_class_name": visual, "item_asset_path": path,
                "descriptor_captured": "yes" if path and f"{path}.json" in have else "no" if path else "",
                "image_captured": "yes" if path and (f"{path}.webp" in have or f"{path}.png" in have) else "no" if path else "",
            })
    path_class = {value: key for key, value in art.items()}
    captured = []
    for picture in sorted(p for p in (raw / "assets").rglob("*") if p.suffix in (".webp", ".png")):
        relative = picture.relative_to(raw / "assets").as_posix()
        stem = relative[: relative.rindex(".")]
        visual = path_class.get(stem, "")
        owners = classes.get(visual, [])
        size = image_size(picture)
        declared = descriptor_size(picture.with_suffix(".json"))
        captured.append({
            "path": relative, "kind": relative.split("/")[0], "category": relative.split("/")[1] if relative.startswith("itemassets/") else "",
            "visual_class_name": visual, "entities": ";".join(owners[:12]), "entity_count": str(len(owners)),
            "width": str(size[0]) if size else "", "height": str(size[1]) if size else "",
            "descriptor_width": str(declared[0]) if declared else "", "descriptor_height": str(declared[1]) if declared else "",
            "size_matches_descriptor": ("yes" if size == declared else "no") if size and declared else "",
            "bytes": str(picture.stat().st_size),
        })
    write_csv(TABLES / "entity_art.csv", ["section", "wodID", "name", "type", "level", "visual_class_name", "item_asset_path", "descriptor_captured", "image_captured"], entity_rows)
    write_csv(TABLES / "captured_images.csv", ["path", "kind", "category", "visual_class_name", "entities", "entity_count", "width", "height", "descriptor_width", "descriptor_height", "size_matches_descriptor", "bytes"], captured)

    write_csv(TABLES / "units.csv", UNIT_LEAD, units)
    write_csv(TABLES / "unit_art.csv", ["wodID", "type", "name", "display_name", "visual_class_name", "item_asset_path", "item_asset_version", "descriptor_captured", "image_captured"], unit_art)
    write_csv(TABLES / "kingdoms.csv", KINGDOM_LEAD, kingdoms)
    write_csv(TABLES / "minute_skips.csv", ["MST", "currencyID", "MinuteSkipIndex", "MinutesSkipValue"], skips)
    (TABLES / "meta.json").write_text(
        json.dumps(
            {
                "items_version": version,
                "items_file": items_path.name,
                "items_sha256": sha256(items_path),
                "texts_file": f"{lang_path.parent.name}/{lang_path.name}",
                "texts_sha256": sha256(lang_path),
                "rows": {"units": len(units), "kingdoms": len(kingdoms), "minute_skips": len(skips)},
                "named_units": sum(1 for u in units if u["display_name"]),
                "art_table_entries": len(art),
                "units_with_art": sum(1 for r in unit_art if r["item_asset_path"]),
                "captured_images": len(captured),
                "ggs_dll_file": dll.name if dll else None,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    print(
        f"items {version}: {len(units)} units ({sum(1 for u in units if u['display_name'])} named, "
        f"{sum(1 for u in units if u['legacy_name'])} with a legacy name), {len(kingdoms)} kingdoms, {len(skips)} skips; "
        f"art for {sum(1 for r in unit_art if r['item_asset_path'])} units from a table of {len(art)}; "
        f"{len(captured)} captured pictures ({sum(1 for r in captured if r['entity_count'] != '0')} tied to entities, "
        f"{sum(1 for r in captured if r['size_matches_descriptor'] == 'yes')} sizes match their descriptor)"
    )

    if rust:
        write_rust(version, units, kingdoms, skips)


def write_rust(version: str, units: list[dict], kingdoms: list[dict], skips: list[dict]) -> None:
    header = (
        "// @generated by utils/scripts/build_tables.py -- do not edit by hand.\n"
        f"// Source: data/tables/*.csv, built from the game's items file version {version}.\n"
        "// Re-run: python3 utils/scripts/build_tables.py\n\n"
    )
    lines = [header + "use crate::units::{Role, Unit};\n", "/// Every unit and tool the game defines, sorted by `wod_id`.",
             f"pub static UNITS: &[Unit] = &[  // {len(units)} rows"]
    for u in units:
        role = {"melee": "Some(Role::Melee)", "ranged": "Some(Role::Ranged)"}.get(u.get("role", ""), "None")
        numbers = ", ".join(f"{field}: {rs_opt_int(u.get(game, ''))}" for field, game in UNIT_NUMBERS)
        lines.append(
            f"    Unit {{ wod_id: {int(u['wodID'])}, type_name: {rs_str(u.get('type', ''))}, "
            f"display_name: {rs_opt_str(u['display_name'])}, legacy_name: {rs_opt_str(u['legacy_name'])}, role: {role}, "
            f"comment1: {rs_str(u.get('comment1', ''))}, comment2: {rs_str(u.get('comment2', ''))}, {numbers} }},"
        )
    lines += ["];", ""]
    (GENERATED / "units.rs").write_text("\n".join(lines), encoding="utf-8")

    out = [header + "use crate::rules::{KingdomRule, MinuteSkip};\n", "/// Every kingdom row of the items file, sorted by `k_id`.",
           "pub static KINGDOM_RULES: &[KingdomRule] = &["]
    for k in kingdoms:
        numbers = ", ".join(f"{field}: {rs_opt_int(k.get(game, ''))}" for field, game in KINGDOM_NUMBERS)
        out.append(
            f"    KingdomRule {{ k_id: {int(k['kID'])}, kingdom_name: {rs_str(k.get('kingdomName', ''))}, "
            f"display_name: {rs_opt_str(k['display_name'])}, legacy_name: {rs_opt_str(k['legacy_name'])}, {numbers} }},"
        )
    out += ["];", "", "/// The minute-skip items, shortest first; `mst` is the key the `msk` packet carries.", "pub static MINUTE_SKIPS: &[MinuteSkip] = &["]
    for s in skips:
        out.append(
            f"    MinuteSkip {{ mst: {rs_str(s['MST'])}, currency_id: {int(s['currencyID'])}, "
            f"minute_skip_index: {int(s['MinuteSkipIndex'])}, minutes_skip_value: {int(s['MinutesSkipValue'])} }},"
        )
    out += ["];", ""]
    (GENERATED / "rules.rs").write_text("\n".join(out), encoding="utf-8")
    print("wrote crates/empire-game/src/generated/units.rs and rules.rs")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("source", nargs="?", type=Path, default=RAW)
    parser.add_argument("--no-rust", action="store_true")
    args = parser.parse_args()
    build(args.source, rust=not args.no_rust)


if __name__ == "__main__":
    main()
