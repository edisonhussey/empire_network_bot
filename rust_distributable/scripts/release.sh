#!/usr/bin/env bash
# Build a release and put the artifacts where they are actually shipped from.
#
# Releases are built from a clean Rust target and clean npm dependency tree,
# then collected in `releases/`. This script is the only supported way to cut
# one, so a developer's incremental target cannot accidentally become the DMG.
#
#   scripts/release.sh                 build, collect, install
#   scripts/release.sh --no-build      collect what is already in target/
#   OUTPUT_DIR=/somewhere scripts/release.sh
#
# Written for bash 3.2: no mapfile, no bare "${arr[@]}" under `set -u`.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP="$ROOT/desktop"
BUNDLE="$DESKTOP/src-tauri/target/release/bundle"
BIN="$DESKTOP/src-tauri/target/release/empire-desktop"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT/releases}"
APP_DEST="/Applications/OpenAuto.app"

BUILD=1
if [ "${1:-}" = "--no-build" ]; then
    BUILD=0
fi

# ------------------------------------------------------------------ version --
#
# `VERSION` at the repo root is the one place to edit. It is copied into the
# files below first, then they are checked anyway: a half-finished bump ships a
# dmg whose filename disagrees with the window inside it, and an unbumped
# API_VERSION makes a *stale service* serve the new window's requests (the
# failure mode that once had a fortress task attacking RBCs for a whole run).
# API_VERSION is a separate contract number and is never synced from VERSION.
"$ROOT/scripts/sync-version.sh"

json_version() {
    python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$1"
}

TAURI_VERSION="$(json_version "$DESKTOP/src-tauri/tauri.conf.json")"
PKG_VERSION="$(json_version "$DESKTOP/package.json")"
CARGO_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$DESKTOP/src-tauri/Cargo.toml" | head -1)"
LOCK_VERSIONS="$(python3 -c '
import json, sys
d = json.load(open(sys.argv[1]))
print(d["version"], d["packages"][""]["version"])
' "$DESKTOP/package-lock.json")"
API_VERSION="$(sed -n 's/^pub const API_VERSION: u16 = \(.*\);/\1/p' \
    "$ROOT/crates/empire-daemon/src/lib.rs" | head -1)"

bad=0
check() {
    if [ "$2" != "$3" ]; then
        echo "version mismatch: $1 says '$2', expected '$3'" >&2
        bad=1
    fi
}
check "package.json" "$PKG_VERSION" "$TAURI_VERSION"
check "Cargo.toml" "$CARGO_VERSION" "$TAURI_VERSION"
for v in $LOCK_VERSIONS; do
    check "package-lock.json" "$v" "$TAURI_VERSION"
done
if [ -z "$API_VERSION" ]; then
    echo "could not read API_VERSION from empire-daemon/src/lib.rs" >&2
    bad=1
fi
if [ "$bad" -ne 0 ]; then
    echo "refusing to release: fix the version files first" >&2
    exit 1
fi

# -------------------------------------------------------------------- build --
if [ "$BUILD" -eq 1 ]; then
    echo "==> clean-building $TAURI_VERSION"
    (cd "$DESKTOP" && cargo clean --manifest-path src-tauri/Cargo.toml)
    (cd "$DESKTOP" && npm ci)
    (cd "$DESKTOP" && npm run tauri build)
fi

DMG="$BUNDLE/dmg/OpenAuto_${TAURI_VERSION}_aarch64.dmg"
APP="$BUNDLE/macos/OpenAuto.app"
if [ ! -f "$DMG" ]; then
    echo "no dmg at $DMG" >&2
    echo "a --no-build run can only collect a version that was already built" >&2
    exit 1
fi
if [ ! -d "$APP" ]; then
    echo "no app bundle at $APP" >&2
    exit 1
fi

# A distributable is a first-run application. User state, licence activation,
# passwords and scan databases belong under Application Support and must never
# be packaged into the app bundle.
embedded_state="$(find "$APP" -type f \( -name '*.sqlite3' -o -name '*.sqlite3-wal' -o -name '*.sqlite3-shm' -o -name '*.token' \) -print -quit)"
if [ -n "$embedded_state" ]; then
    echo "refusing to release app bundle containing user state: $embedded_state" >&2
    exit 1
fi

# ----------------------------------------------------------------- staleness --
#
# Two ways a release looks fine and is not one. Neither is fatal, so they warn
# rather than stop: a comment-only edit leaves the source newer than the binary
# and still produces a correct build.
newer="$(find "$ROOT/crates" "$DESKTOP/src-tauri/src" -name '*.rs' -newer "$BIN" 2>/dev/null | head -5)"
if [ -n "$newer" ]; then
    echo "WARNING: source is newer than the binary; this dmg does not contain:" >&2
    echo "$newer" >&2
fi

# Tauri embeds the frontend compressed, so a stale UI cannot be found by
# grepping the binary — only by comparing timestamps.
asset="$(ls -t "$DESKTOP"/dist/assets/*.js 2>/dev/null | head -1 || true)"
if [ -n "$asset" ] && [ "$asset" -nt "$BIN" ]; then
    echo "WARNING: frontend asset is newer than the binary; the shipped UI is the previous build" >&2
fi

# ------------------------------------------------------------------ collect --
mkdir -p "$OUTPUT_DIR"
cp -f "$DMG" "$OUTPUT_DIR/"

# ------------------------------------------------------------------ install --
osascript -e 'quit app "OpenAuto"' >/dev/null 2>&1 || true
sleep 1
for pid in $(pgrep -f '^/Applications/OpenAuto.app/Contents/MacOS/empire-desktop( --service)?$' || true); do
    kill "$pid" 2>/dev/null || true
done
ditto "$APP" "$APP_DEST"

# ------------------------------------------------------------------- report --
echo
echo "version   $TAURI_VERSION   (API_VERSION $API_VERSION)"
echo "dmg       $OUTPUT_DIR/$(basename "$DMG")  $(ls -lh "$DMG" | awk '{print $5}')"
echo "md5       $(md5 -q "$BIN")"
echo "installed $(defaults read "$APP_DEST/Contents/Info.plist" CFBundleShortVersionString 2>/dev/null || echo '?')"
echo
echo "A service swap drops the live session — reconnect from Start."
