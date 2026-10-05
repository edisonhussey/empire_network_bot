#!/usr/bin/env bash
# Copy the version in `VERSION` (repo root) into every file that must agree
# with it: tauri.conf.json, package.json, package-lock.json, src-tauri/Cargo.toml.
# Idempotent. release.sh runs this first, so editing VERSION is all a bump needs.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP="$ROOT/desktop"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"

case "$VERSION" in
    [0-9]*.[0-9]*.[0-9]*) ;;
    *) echo "VERSION must look like 1.2.3, got '$VERSION'" >&2; exit 1 ;;
esac

python3 - "$VERSION" "$DESKTOP" <<'PY'
import json, re, sys
version, desktop = sys.argv[1], sys.argv[2]

def edit_json(path, apply):
    with open(path) as f:
        data = json.load(f)
    apply(data)
    with open(path, "w") as f:
        json.dump(data, f, indent=2)
        f.write("\n")

def tauri(d): d["version"] = version
def package(d): d["version"] = version
def lock(d):
    d["version"] = version
    d["packages"][""]["version"] = version

edit_json(f"{desktop}/src-tauri/tauri.conf.json", tauri)
edit_json(f"{desktop}/package.json", package)
edit_json(f"{desktop}/package-lock.json", lock)

cargo = f"{desktop}/src-tauri/Cargo.toml"
text = open(cargo).read()
text = re.sub(r'(?m)^version = "[^"]*"', f'version = "{version}"', text, count=1)
open(cargo, "w").write(text)
PY

echo "version files set to $VERSION"
