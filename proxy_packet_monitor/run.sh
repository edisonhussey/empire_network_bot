#!/bin/bash
# Start the dashboard. Ports come from .env (read by the server itself; a real environment variable wins).
# Used by process_monitor and by hand.
cd "$(dirname "$0")" || exit 1
ulimit -n 10240 2>/dev/null  # launchd children default to 256; mitmdump needs many sockets
PY="${PPM_PYTHON:-$(sed -n 's/^PPM_PYTHON=//p' .env 2>/dev/null | tail -1)}"
for candidate in .venv/bin/python ../venv/bin/python; do
  [ -n "$PY" ] || { [ -x "$candidate" ] && PY="$candidate"; }
done
exec "${PY:-python3}" -m backend.web_server
