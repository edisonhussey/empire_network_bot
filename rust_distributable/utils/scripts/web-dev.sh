#!/usr/bin/env bash
# Web development loop: the daemon (rebuilt on every Rust change) plus the Vite dev server (live UI reload).
#
#   utils/scripts/web-dev.sh          daemon under cargo-watch + Vite on http://localhost:1420
#   utils/scripts/web-dev.sh ui       Vite only (use the daemon that is already running)
#   utils/scripts/web-dev.sh daemon   daemon under cargo-watch only
#
# The daemon owns every bot. The page (Vite tab, the daemon's own page on :47821, or the Tauri window) only
# starts, stops and changes them, so closing a tab stops nothing. Only one daemon can hold :47821: stop the
# github_gge_bot card in process_monitor (http://127.0.0.1:8000) or quit OpenAuto's service first.
# Restarting the daemon (every Rust save) drops its live game sessions; bots resume as they do after any restart.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PORT="${EMPIRE_BIND:-127.0.0.1:47821}"
MODE="${1:-all}"

port_busy() { lsof -nP -iTCP:"${PORT##*:}" -sTCP:LISTEN -t >/dev/null 2>&1; }

run_daemon() {
    if port_busy; then
        echo "Something already listens on $PORT:" >&2
        lsof -nP -iTCP:"${PORT##*:}" -sTCP:LISTEN >&2 || true
        echo "Stop it first (process_monitor card github_gge_bot, or the OpenAuto service), or run '$0 ui' to use it." >&2
        exit 1
    fi
    cd "$ROOT"
    if command -v cargo-watch >/dev/null 2>&1; then
        # Watch only what the daemon is built from; the UI and docs do not restart it.
        exec cargo watch -q -c -w crates -w Cargo.toml -x "run -p empire-daemon"
    fi
    echo "cargo-watch is not installed (cargo install cargo-watch); running once without restarts." >&2
    exec cargo run -p empire-daemon
}

run_ui() {
    cd "$ROOT/desktop"
    [ -d node_modules ] || npm install
    exec npm run dev
}

case "$MODE" in
    ui) run_ui ;;
    daemon) run_daemon ;;
    all)
        if port_busy; then run_daemon; fi   # prints who holds the port and exits
        ( run_daemon ) &
        DAEMON=$!
        trap 'kill "$DAEMON" 2>/dev/null || true' EXIT INT TERM
        ( run_ui )
        ;;
    *) sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
