# Proxy Packet Monitor

A standalone, receive-only recorded-session explorer. The code and all generated configuration/data live under this directory. `reference/` contains unchanged copies of the existing listener, packet parser, injection bridge, and WebSocket endpoint matcher for comparison. The active monitor uses the same `ep-live-*-game.goodgamestudios.com` WebSocket rule and local proxy port `8080`, but does not run the bot's attack control loop.

## Run

From the repository root:

```sh
venv/bin/python -m pip install -r proxy_packet_monitor/requirements.txt
cd proxy_packet_monitor/desktop
npm install
npm run tauri dev
```

The Rust desktop launches the Python loopback service. For a fast browser-based development loop, run `../venv/bin/python -m backend.server` from `proxy_packet_monitor/` in one terminal and `npm run dev` from `proxy_packet_monitor/desktop/` in another. Open `http://127.0.0.1:1421`. Click **Start proxy**, point the game client at `127.0.0.1:8080` using the same mitmproxy certificate setup as the existing listener, then use **Start recording** or the global `Ctrl+Alt+X` shortcut. On macOS, grant Accessibility/Input Monitoring permission to the Python process for the global shortcut. The shortcut is configurable through Settings or `config.json` and takes effect after restart.

Sessions open automatically at startup, newest first. A session is a versioned SQLite file in `data/sessions/`. The file stores the original decoded text and parsed XT structure, plus sequence, timestamp, type and summary. Filtering never changes it. Export writes a complete JSON file in `data/`, independent of the current filter. After a crash, the service finalizes healthy `.partial.sqlite3` files on startup and marks them `recovered` in metadata.

The API is bound to `127.0.0.1:8798` by default. Useful routes: `GET /api/status`, `POST /api/proxy/start`, `POST /api/proxy/stop`, `POST /api/recording/toggle`, `GET /api/sessions`, `GET /api/sessions/{id}/events?type=ADI&limit=100&offset=0`, `GET /api/sessions/{id}/events/{seq}`, `POST /api/sessions/{id}/export`, and `DELETE /api/sessions/{id}`. This local API is the extension point for future command development; no send action is exposed yet.
