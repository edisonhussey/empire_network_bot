# Proxy Packet Monitor

A standalone, bidirectional recorded-session explorer with an experimental incoming movement tab. The code and all generated configuration/data live under this directory. `reference/` contains unchanged copies of the existing listener, packet parser, injection bridge, and WebSocket endpoint matcher for comparison. The active monitor uses the same `ep-live-*-game.goodgamestudios.com` WebSocket rule and local proxy port `8080`, but does not run the bot's attack control loop.

## Run the web dashboard

From the repository root:

```sh
cd proxy_packet_monitor
../venv/bin/python -m backend.web_server
```

Open **http://127.0.0.1:8799** in a browser. The Python server serves plain HTML, CSS and JavaScript directly from `web/`; editing those files only needs a browser refresh. The **Recorded sessions** tab reads the existing completed-session folder. The new **Incoming** tab lets you set your own kingdom and castle coordinates, starts with an empty in-memory ledger, and updates from incoming `GAM` messages whether or not session recording is on. Click **Start proxy** in the page, then point the game client at the experimental proxy, `127.0.0.1:8080`, using the existing mitmproxy certificate setup. The original service remains on `8798`. Only one proxy can bind `8080` at a time; if the original proxy is running, stop that process or choose another port in the web dashboard Settings and use that same port in the game client.

For the optional global recording shortcut, install `requirements.txt` into the existing virtual environment. The web dashboard and Incoming tab work without that dependency.

The separate `backend/web_server.py` runs `backend/incoming_addon.py`. Both proxy addons record client-sent and server-received game WebSocket frames while recording is on. Only server-received `GAM` messages feed the Incoming ledger; that observation works whether session recording is on or off. Completed sessions are saved in the shared session folder. Its control and status files live separately in `web_data/`. Predicted arrival uses the first observed `TT − PT` value. A matching GAM movement is labelled an incoming candidate because the hostile/support distinction is not yet established. The supplied GAM sample has no verified troop or tool formation fields, so composition is shown as **Unknown**.

The field-by-field parsing notes and open questions are in [learned_info/incoming_gam.md](learned_info/incoming_gam.md).

Run the focused checks with:

```sh
PYTHONPATH=. ../venv/bin/python -m unittest discover -s tests -v
```

## Optional compiled desktop

From the repository root:

```sh
venv/bin/python -m pip install -r proxy_packet_monitor/requirements.txt
cd proxy_packet_monitor/desktop
npm install
npm run tauri dev
```

The older Rust desktop remains available for the recorded-session inspector. The web page above is the fastest way to use the new Incoming tab. On macOS, grant Accessibility/Input Monitoring permission to the Python process for the optional global shortcut. The shortcut is configurable through Settings or `config.json` and takes effect after restart.

Sessions open automatically at startup, newest first. A session is a versioned SQLite file in `data/sessions/`. The file stores the original decoded text and parsed XT or XML structure, plus sequence, timestamp, type, direction (`sent` or `received`), and summary. Existing receive-only sessions remain readable and their events display as received. Filtering never changes it. Export writes a complete JSON file in `data/`, independent of the current filter. After a crash, the service finalizes healthy `.partial.sqlite3` files on startup and marks them `recovered` in metadata.

The original desktop API is bound to `127.0.0.1:8798`; the web experiment uses `127.0.0.1:8799`. The web API adds `GET /api/incoming` and `POST /api/incoming/config` to the existing session and proxy-control routes. Its addon posts observed GAM frames to `POST /api/incoming/observe`. No send action is exposed.
