#!/usr/bin/env bash
# Issue a 30-day OpenAuto token. Usage: admin/generate_token.sh NAME SERVER X Y
#   NAME    customer label (also the licence id), e.g. alice
#   SERVER  US1 or WORLD2
#   X Y     the player's main-castle coordinates on that server
# Prints the token and saves it to admin/NAME-30-day.token (mode 600).
set -euo pipefail
[ "$#" -eq 4 ] || { sed -n '2,6p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 1; }
NAME="$1"; SERVER="$2"; X="$3"; Y="$4"
ADMIN="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$ADMIN/$NAME-30-day.token"
[ ! -e "$OUT" ] || { echo "$OUT already exists; pick another NAME" >&2; exit 1; }
cd "$ADMIN/.."
cargo run -q -p empire-license-admin -- issue \
    --private "$ADMIN/master_key.txt" \
    --license-id "$NAME" --subject "$NAME" \
    --server "$SERVER" --bootstrap-x "$X" --bootstrap-y "$Y" \
    --days 30 --output "$OUT" >/dev/null
cat "$OUT"
