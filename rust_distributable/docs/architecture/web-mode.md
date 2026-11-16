# Web mode: a browser tab as the control panel

The same UI runs in the Tauri window and in a plain browser tab. Both are only control panels: the daemon
(`empire-daemon`, or the app's `--service` process) owns every bot and every game session, so closing the
window or the tab stops nothing. This is the dev loop for UI work and the base for the coming UI redesign.

## The three ways to open the UI

| Host | URL | Talks to | Used for |
| --- | --- | --- | --- |
| Vite dev server | `http://localhost:1420` | `http://127.0.0.1:47821/v1` (CORS allows :1420) | UI development: save a file, the page reloads |
| The daemon's own page | `http://127.0.0.1:47821/` | its own origin, `/v1` | everyday use from a browser; what process_monitor links to |
| Tauri window | `tauri://localhost` | `http://127.0.0.1:47821/v1` | the installed app, unchanged |

## Who owns what

* `crates/empire-daemon/src/web_ui.rs`: serves the built UI (`desktop/dist`) as the router's fallback, so every `/v1`
  route still wins. The folder is `EMPIRE_WEB_DIR`, else `desktop/dist` under the working directory. No folder means no
  page (the app's `--service` process runs from `/`, so it serves none and behaves as before). Paths that try to leave the
  folder are refused (`404`).
* `desktop/src/platform.js`: the one place that knows which host the UI runs in. It decides the API base
  (`VITE_EMPIRE_API` override; Tauri and Vite use the fixed port; a page the daemon served uses its own origin) and wraps the
  host-specific calls (`copyText`, `notify`). Anything Tauri-only added later (native dialogs, `invoke`) goes here, behind
  a function that has a browser fallback, so the rest of the UI never checks the host.
* `desktop/src/api.js`: unchanged role, it just asks `platform.js` for the base.

Nothing in the UI used Tauri APIs before this (no `invoke`, no Tauri plugins): it was already plain `fetch` to the daemon, and
the daemon was already its own long-lived process. So no workaround was needed; the change only removed the hard-coded
URL and gave the page a server.

## Dev loop

```sh
oa web            # daemon under cargo-watch (restarts on any save in crates/) + Vite on http://localhost:1420
oa web ui         # Vite only, using the daemon that is already running (process_monitor's, or the app's)
oa web daemon     # the daemon under cargo-watch only
```

* UI change: save, the Vite tab reloads. No Rust build, no app bundle.
* Rust change: cargo-watch rebuilds and restarts the daemon (`cargo install cargo-watch` once; without it the script runs
  the daemon once and says so). A restart drops the live game sessions, exactly like any daemon restart.
* Only one daemon can hold `:47821`. The script refuses to start a second one and names who holds the port: stop the
  `github_gge_bot` card in process_monitor (`http://127.0.0.1:8000`) or quit the app's service first.
* The daemon's own page shows the last `npm run build` (`desktop/dist`), not live edits. Rebuild it after UI changes you want
  there, and rebuild `target/release/empire-daemon` after Rust changes before process_monitor starts it.

## process_monitor

The local panel (`~/Desktop/process_monitor`, a separate project) already starts and stops the daemon as the
`github_gge_bot` card (`./target/release/empire-daemon` from `rust_distributable/`). Its link now points at
`http://127.0.0.1:47821/` instead of `/v1/health`, so the card opens the control panel. Because the panel spawns the daemon
in its own process group, the daemon outlives both the panel page and the bot page.

## Not done (kept simple on purpose)

* No WebSocket push to the UI: it polls, as it did in the window. A push channel can be added later behind `api.js`.
* No login or token on the page: the API is loopback-only, as before ([`security.md`](security.md)). Do not bind it to a
  public address.
