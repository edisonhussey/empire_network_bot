# Proxy Packet Monitor

A standalone, bidirectional recorded-session explorer with an experimental incoming movement tab. The code and all generated configuration/data live under this directory. `reference/` contains unchanged copies of the existing listener, packet parser, injection bridge, and WebSocket endpoint matcher for comparison. The active monitor uses the same `ep-live-*-game.goodgamestudios.com` WebSocket rule and local proxy port `8080`, but does not run the bot's attack control loop.

## Run the web dashboard

**Ports and paths live in [.env](.env)** (`PPM_WEB_PORT`, `PPM_PROXY_PORT`, optional `PPM_MITMDUMP`/`PPM_PYTHON`); change a port there and restart, nothing else needs editing. A real environment variable of the same name wins, which is how the process_monitor panel on :8000 passes its slot (`apps.toml`, slot 8008). The Settings page also writes ports back to `.env`.

Start it from the panel (**proxy_packet_monitor**, http://127.0.0.1:8000) or by hand:

```sh
cd proxy_packet_monitor
./run.sh
```

Open **http://127.0.0.1:8008** in a browser. The Python server serves plain HTML, CSS and JavaScript directly from `web/`; editing those files only needs a browser refresh. The **Recorded sessions** tab reads the existing completed-session folder. The **Incoming** tab watches the owned castle tuples configured in Auto Support, falling back to its older single-location setting when no tuples are configured. It starts with an empty in-memory ledger and updates from inbound `GAM` and confirmed `MRM` messages whether or not session recording is on. Click **Start proxy** in the page, then point the game client at the experimental proxy, `127.0.0.1:8080`, using the existing mitmproxy certificate setup. The original desktop service remains on `8798`. Only one proxy can bind `8080` at a time; if the original proxy is running, stop that process or choose another port in the web dashboard Settings and use that same port in the game client.

For the optional global recording shortcut, install `requirements.txt` into the existing virtual environment. The web dashboard and Incoming tab work without that dependency.

The separate `backend/web_server.py` runs `backend/incoming_addon.py`. Both proxy addons record client-sent and server-received game WebSocket frames while recording is on. Server-received `GAM` messages feed the Incoming ledger and confirmed `MRM` messages mark known movements retreated; observation works whether session recording is on or off. Completed sessions are saved in the shared session folder. Its control and status files live separately in `web_data/`. Predicted arrival uses the first observed `TT − PT` value. A matching GAM movement is labelled an incoming candidate because the hostile/support distinction is not established by a separate attack flag. When `GA` is present, the tab totals its `L`, `M`, `R`, and `RW` unit counts. `GS` alone, tools, and wave count remain **Unknown**.

The **Auto Support** tab is a separate proxy proof of concept. It starts **off**. Configure the owned source castles as `(KID, X, Y)`, a destination `(X, Y)` for every source kingdom, and the server LID. When enabled, a GAM that matches an owned castle and the alliance rule below schedules `dcl` around seven seconds before estimated landing, `sdi` at five seconds, and `cds` after the support details reply. The proxy intersects the matched DCL castle's `AC` counts with `sdi.gui.I` and the copied known troop ID catalog, then sends up to ten distinct positive troop IDs in descending home quantity. After a confirmed `cds`, it stores the new support movement MID. At estimated attack landing or an earlier inbound `mrm` for that attack, it sends `mcm` for the support MID. A confirmed `mcm` reports the estimated return time from `TT − PT`. Pending support recalls survive proxy restarts and remain active even if the Auto Support toggle is turned off. On startup, the proxy checks recorded successful MCM replies to reconcile an uncertain recall, including one the player sent manually. An uncertain `mcm` is not repeated automatically; a missing local XT header is retried because no injection was attempted. Recent attempt states appear in the tab. The new API is `GET /api/auto-support` and `POST /api/auto-support/config`; settings live in `web_data/auto_support.json`.

The Auto Support trigger requires both a configured owned castle target and different sender/receiver alliance IDs, resolved from GAM `O[]` by player ID. Equal IDs or missing player/alliance records are skipped. This is the requested operational rule; GAM itself does not supply a separately verified attack-type flag. Two players without alliances both report `AID=-1`, so this rule also skips their movements. This proxy experiment does not have the Rust bot's LID reservation or scheduler and does not guarantee arrival-relative timing when the server is slow or the estimated landing is imprecise. It blocks a second support job using the same LID while a support march is active or its return is unconfirmed. Avoid running another attack controller against the same account/LID during a proof-of-concept trial. Packet findings and limits are in [learned_info/incoming_gam.md](learned_info/incoming_gam.md), [dcl.md](learned_info/dcl.md), [sdi.md](learned_info/sdi.md), [cds.md](learned_info/cds.md), and [mcm.md](learned_info/mcm.md).

The field-by-field parsing notes and open questions are in [learned_info/incoming_gam.md](learned_info/incoming_gam.md).

**Download login data** (checkbox in the toolbar, off by default). While it is on, the proxy keeps the game's own downloads from `PPM_CAPTURE_HOSTS` (items, config, language bundles; images, audio and fonts are skipped) under `web_data/login_data/`. Files go to `files/<host>/<url path>`, identical bodies are stored once, and a changed body at the same path is kept as `<name>.<sha8>`. `index.jsonl` has one line per download (`t`, `url`, `type`, `size`, `sha256`, `file`, `ws_seq`) plus a `{"event":"websocket"}` line each time the game opens its WebSocket, so a login can be searched offline with a few lines of Python instead of reading the files. Log in through the proxy after ticking it; nothing is saved from before.

Run the focused checks with:

```sh
PYTHONPATH=. ../venv/bin/python -m unittest discover -s tests -v
```

**Inject** (box under the tabs on the home screen). Paste a frame such as `%xt%EmpireEx_19%nch%1%` and press Send (or Ctrl/Cmd+Enter) to push it through the live game WebSocket, to see the effect in the game at once. One frame per line, up to 20 at a time; frames are sent exactly as pasted. *To server* is what the game client would send; *To client* makes the game think the server sent it. The box says whether a game WebSocket is connected, and a refusal ("No active game WebSocket", "proxy is not running") is shown in the notice. Injected frames are recorded in the session like any other, but a frame injected *to the client* is not shown to Auto Support or the Incoming ledger, so a test `gam` cannot trigger a real support march. The page and proxy talk through `inject.json` / `inject_result.json` in the data folder (`backend/injector.py`); after updating, restart the page and stop/start the proxy once so the running proxy loads the new code.

**Reading replies while recording.** An active recording is a `*.partial.sqlite3` that is committed to disk only every 100 events, so a reply injected a moment ago is not in the file yet. Stopping the recording flushes and finalises it (the file loses `.partial`); to read the result of an injected frame, stop, read, then start again.

**Recording shortcut** (Settings, "shortcut"; set to `space` now). One key, **in any application**, toggles recording (`● Start` / `■ Stop`), so a short snippet becomes its own session without looking at the page. It listens **only while the proxy is running** (started and stopped with it; the header says "listening" or "starts with the proxy"), and a held key flips it once. Names accepted: `space`, `f9`, `esc`, `enter`, `ctrl+alt+r`, a single character, or pynput's `<space>`. Two things to know:
* The key is *listened to, not swallowed*: it still types in whatever you are using. With `space` (or a letter), **every space you type anywhere while the proxy runs also toggles recording**, including in the game's own chat. Pick a key you do not type (`f9`, `ctrl+alt+r`) if that gets in the way, or clear the setting to switch it off.
* macOS only delivers the key if the program that launched the monitor (Terminal, or the process monitor) is allowed under System Settings, Privacy and Security, **Input Monitoring**. Without it nothing arrives and no error is shown.
A recording in progress is a `.partial` file that is committed only every 100 events, so a snippet is readable (by you or by anyone querying the folder) once it is **stopped**; the newest session is then the one just recorded.

**Export visible (raw)** (button under the session list). Writes the events the timeline shows right now, meaning your type filters and search together, as raw text, **one event per line**, to `<data_dir>/exports/<session>_<filters>.txt`, and copies it to `<data_dir>/exports/latest.txt` so the newest export always has the same path (the path is also copied to your clipboard). A line break inside a frame (rare, some XML frames) is written as `\n`. The same text is available as a download from `GET /api/sessions/<id>/raw?type=KPI&q=...`.

**Include images** (checkbox beside Download login data, off by default). Also keeps the pictures the game loads (png, jpg, gif, webp, svg) with the other login downloads, under `files/<host>/<path>`; audio, video and fonts are never kept. Needs Download login data on, and a hard reload of the game so it fetches the pictures again. The header shows the megabytes saved so far. Pictures load as they are used, so one login saves what that login showed; sort them into the project with `utils/scripts/organize_game_data.py` (in `rust_distributable`).

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

The original desktop API is bound to `127.0.0.1:8798`; the web experiment uses `127.0.0.1:8008` (`PPM_WEB_PORT`). The web API adds `GET /api/incoming`, `POST /api/incoming/config`, and the Auto Support configuration routes to the existing session and proxy-control routes. Its addon posts observed GAM frames to `POST /api/incoming/observe`. The Auto Support addon injects support packets when enabled and outstanding support recalls after dispatch, including when the toggle is off.
