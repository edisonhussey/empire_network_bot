#!/usr/bin/env python3
"""Copy a "Download login data" capture into ``data/raw/game_files/``, sorted by what each file does.

The capture is what ``proxy_packet_monitor`` saves while its "Download login data" box is ticked
(``proxy_packet_monitor/web_data/login_data/``: a ``files/<host>/<path>`` tree plus ``index.jsonl``).
This copies it (the capture is never touched) into folders named for the file's job. A file that
matches no rule goes to ``unsorted/`` rather than being guessed at.

    python3 utils/scripts/organize_game_data.py                  # default source and destination
    python3 utils/scripts/organize_game_data.py --dry-run        # show the plan, copy nothing
    python3 utils/scripts/organize_game_data.py SRC [DEST]

Layout produced (the manual is ``docs/game_data/README.md``):

    client/       the game's own program: index.html, the JavaScript bundles, ggs.dll, styles, worker
    rules/items/  the game's rule tables (items_v<version>.json and its version file)
    languages/    texts (en.json), locale formats (cldr), the language-server snippets
    config/       servers, countries, branches
    assets/       art descriptors: interface/ and itemassets/
    unsorted/     everything else, under its original host and path
    manifest.jsonl   one line per file: where it went, where it came from, size, sha256

Re-running is safe: a file already there with the same size is skipped. After a game update, capture
again, run this, then ``utils/scripts/build_tables.py`` to rebuild the lookup tables and the Rust.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
from collections import Counter
from pathlib import Path
from urllib.parse import urlsplit

RUST_DIR = Path(__file__).resolve().parents[2]
DEFAULT_SOURCE = RUST_DIR.parent / "proxy_packet_monitor" / "web_data" / "login_data"
DEFAULT_DEST = RUST_DIR / "data" / "raw" / "game_files"

HTML5 = "empire-html5.goodgamestudios.com"
LANGSERV = "langserv.public.ggs-ep.com"


def classify(host: str, path: str) -> str:
    """The destination (relative to the game_data root) for one downloaded file."""
    parts = path.split("/")
    name = parts[-1]
    if host == HTML5:
        if path.startswith("default/items/"):
            return "rules/items/" + name
        if path.startswith("config/languages/"):
            return "languages/texts/" + path[len("config/languages/"):]
        if path.startswith("default/cldr/"):
            return "languages/cldr/" + path[len("default/cldr/"):]
        if path.startswith("config/network/"):
            return "config/network/" + path[len("config/network/"):]
        if path.startswith("default/config/"):
            return "config/" + path[len("default/config/"):]
        if path.startswith("default/dll/"):
            return "client/dll/" + name
        if path.startswith("default/css/"):
            return "client/css/" + name
        if path.startswith("default/assets/itemassets/"):
            return "assets/itemassets/" + path[len("default/assets/itemassets/"):]
        if path.startswith("default/assets/interface/"):
            return "assets/interface/" + path[len("default/assets/interface/"):]
        if len(parts) == 2 and parts[0] == "default" and (
            name == "index.html" or ".bundle." in name or name.endswith(".worker.js")
        ):
            return "client/" + name
    if host == LANGSERV:
        return "languages/langserv/" + path
    return f"unsorted/{host}/{path}"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("source", nargs="?", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("dest", nargs="?", type=Path, default=DEFAULT_DEST)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    files_root = args.source / "files"
    if not files_root.is_dir():
        sys.exit(f"no capture at {files_root} (tick 'Download login data' in proxy_packet_monitor and log in)")

    index = {}
    index_path = args.source / "index.jsonl"
    if index_path.exists():
        for line in index_path.read_text(encoding="utf-8").splitlines():
            row = json.loads(line)
            if row.get("file"):
                index[Path(row["file"]).as_posix().split("files/", 1)[-1]] = row

    manifest, sizes, counts, copied = [], Counter(), Counter(), 0
    planned: set[str] = set()

    def place(source: Path, relative: str, target: str, row: dict, duplicate_of: str | None = None) -> None:
        nonlocal copied
        category = target.split("/", 1)[0]
        size = source.stat().st_size
        sizes[category] += size
        counts[category] += 1
        planned.add(target)
        destination = args.dest / target
        if not args.dry_run and not (destination.exists() and destination.stat().st_size == size):
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
            copied += 1
        manifest.append({
            "path": target,
            "category": category,
            "from": relative,
            "duplicate_of": duplicate_of,
            "url": row.get("url"),
            "content_type": row.get("type"),
            "size": size,
            "sha256": row.get("sha256") or sha256(source),
        })

    for source in sorted(p for p in files_root.rglob("*") if p.is_file()):
        relative = source.relative_to(files_root).as_posix()
        host, _, path = relative.partition("/")
        place(source, relative, classify(host, path), index.get(relative, {}))

    # The capture stores identical bytes once, however many URLs returned them. Give every captured URL its own path here,
    # so "is this picture in the copy?" can be answered by its path.
    for relative, row in index.items():
        host, path = row.get("host"), urlsplit(row.get("url", "")).path.lstrip("/")
        if not host or not path:
            continue
        target = classify(host, path)
        source = files_root / relative
        if target not in planned and source.is_file():
            place(source, relative, target, row, duplicate_of=relative)

    if not args.dry_run:
        args.dest.mkdir(parents=True, exist_ok=True)
        with (args.dest / "manifest.jsonl").open("w", encoding="utf-8") as out:
            for row in manifest:
                out.write(json.dumps(row) + "\n")
    print(f"{'would copy' if args.dry_run else 'copied'} {len(manifest) if args.dry_run else copied} of {len(manifest)} files into {args.dest}")
    for category in sorted(sizes, key=lambda key: -sizes[key]):
        print(f"  {category:10s} {counts[category]:4d} files  {sizes[category] / 1e6:7.2f} MB")


if __name__ == "__main__":
    main()
