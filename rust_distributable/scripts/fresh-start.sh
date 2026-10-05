#!/usr/bin/env bash
#
# Return OpenAuto to a first-run state, so it can be tested as a new user would
# receive it.
#
# Deleting the database is what enforces the two rules a release has to satisfy:
# with no stored licence the app cannot get past the activation screen, so a
# valid session token is always required; and the token it accepts is then bound
# to the account that initialises it, because the licence records the portal
# player id it was first used with.
#
# The existing database is never destroyed - it is copied to a timestamped
# backup first - and only the newest few backups are kept, so repeated test runs
# do not leave dozens of multi-megabyte files behind.
#
#   scripts/fresh-start.sh --dry-run     show what would happen, change nothing
#   scripts/fresh-start.sh               do it
#   KEEP_BACKUPS=5 scripts/fresh-start.sh
set -euo pipefail

DATA_DIR="${HOME}/Library/Application Support/com.openauto.desktop"
DATABASE_NAME="empire.sqlite3"
KEEP_BACKUPS="${KEEP_BACKUPS:-2}"
DRY_RUN=0

case "${1:-}" in
  --dry-run) DRY_RUN=1 ;;
  --help|-h) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  "") ;;
  *) echo "unknown option: $1 (try --help)" >&2; exit 2 ;;
esac

say() { printf '  %s\n' "$*"; }
run() {
  if [ "$DRY_RUN" = "1" ]; then
    say "would: $*"
  else
    "$@"
  fi
}

echo "OpenAuto fresh start"
echo "  data dir: ${DATA_DIR}"
echo

if [ ! -d "$DATA_DIR" ]; then
  say "no data directory yet - the app has never run, nothing to reset"
  exit 0
fi

# The window owns the database through its service process, so it has to be
# stopped first or the file would be recreated mid-reset.
if pgrep -f 'OpenAuto.app/Contents/MacOS|MacOS/empire-desktop' >/dev/null 2>&1; then
  say "stopping OpenAuto"
  run pkill -f 'OpenAuto.app/Contents/MacOS' || true
  run pkill -f 'MacOS/empire-desktop' || true
  sleep 1
else
  say "OpenAuto is not running"
fi

DATABASE="${DATA_DIR}/${DATABASE_NAME}"

if [ -f "$DATABASE" ]; then
  BACKUP="${DATABASE}.before-fresh-start-$(date +%Y%m%d-%H%M%S)"
  say "backing up to $(basename "$BACKUP")"
  # `.backup` goes through SQLite, so a WAL that has not been checkpointed is
  # included; copying the file would not be enough.
  run sqlite3 "$DATABASE" ".backup '${BACKUP}'"
else
  say "no database present"
fi

say "removing the database and its write-ahead log"
run rm -f "$DATABASE" "${DATABASE}-wal" "${DATABASE}-shm"

# Keep the newest N backups and drop the rest. `ls -1t` orders by modification
# time, so the tail of the list is the oldest. Done through a file rather than an
# array because macOS still ships bash 3.2, which has neither `mapfile` nor safe
# empty-array expansion under `set -u`.
BACKUP_LIST="$(mktemp)"
trap 'rm -f "$BACKUP_LIST"' EXIT
ls -1t "${DATABASE}".* 2>/dev/null > "$BACKUP_LIST" || true
BACKUP_COUNT="$(wc -l < "$BACKUP_LIST" | tr -d ' ')"
if [ "$BACKUP_COUNT" -gt "$KEEP_BACKUPS" ]; then
  say "pruning $(( BACKUP_COUNT - KEEP_BACKUPS )) old backup(s), keeping ${KEEP_BACKUPS}"
  tail -n "+$(( KEEP_BACKUPS + 1 ))" "$BACKUP_LIST" | while IFS= read -r old; do
    [ -n "$old" ] && run rm -f "$old"
  done
else
  say "backups to keep: ${BACKUP_COUNT} (limit ${KEEP_BACKUPS})"
fi

echo
if [ "$DRY_RUN" = "1" ]; then
  say "dry run - nothing was changed"
else
  say "done. Next launch opens on the activation screen and asks for a token."
fi
