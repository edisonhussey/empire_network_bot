# OpenAuto — visual layer, extracted

This is **only the user interface** of the OpenAuto desktop app. It is a copy, not
the live tree. Redesign anything in here and the result can be dropped straight
back into the app.

The app is a Tauri shell: Rust owns the database and the game connection, this
frontend is a plain web page that talks to a local HTTP API on
`http://127.0.0.1:47821/v1`. There is no framework, no build step beyond Vite, and
no dependencies — just HTML, one JS file and one CSS file.

---

## The one rule

**The API contract must not change.** The Rust side is fixed and will not be
edited to match a redesign. Every field below already exists and is already
flowing. If a mock-up needs data that is not in these responses, treat it as
optional decoration, or ask — do not invent new endpoints or rename fields.

Base URL: `http://127.0.0.1:47821/v1` (const `API` at the top of `src/main.js`).

| Method | Path | Returns | Drives |
|---|---|---|---|
| GET | `/health` | `{status, api_version, service_pid, licence_active, transport_connected, recent_message_limit}` | sidebar gateway dot |
| GET | `/licence` | `{active, reason, license_id, subject, tier, revision, expires_at, remaining_seconds, features}` | plan badge, activation gate |
| POST | `/licence` | same, body `{token}` | activation form |
| GET | `/accounts` | `AccountSummary[]` | Start tab account card |
| POST | `/accounts` | body `{server, username, password, scan_radius, reuse_existing_map}` | "Start connection" |
| GET | `/direct` | `{connected, phase, endpoint, account_id, error, scan_total, scan_sent, scan_cached, bot_state, bot_detail}` | connection state, bot pill |
| DELETE | `/direct` | — | "Close connection" |
| GET | `/plans` | the whole library (see below) | every builder tab |
| POST | `/plans/attacks` · `/tasks` · `/modes` · `/recruitments` · `/recruit-bots` | `{id}` | the Save buttons |
| DELETE | `/plans/attacks/{id}` · `/tasks/{id}` · `/modes/{id}` · `/recruitments/{id}` · `/recruit-bots/{id}` | 204 | row Delete buttons |
| GET | `/plans/example` | a ready-made mode to import | "Load example" |
| POST | `/plans/import` | body `{json}` | mode import |
| POST | `/plans/start` | body `{account_id, mode_id, recruit_bot_id, running}` | "Start bot" / "End bot" |
| GET | `/hunt` | `HuntSummary` | Live run card |
| GET | `/dashboard` | `{generated_at_ms, attacks_last_hour, returns_last_hour, rubies_last_hour, coins_last_hour, ruby_series, scan_activity}` | Dashboard metrics and chart |
| GET | `/messages` | `{sequence, observed_at_ms, direction, command, payload}[]` | Logs console |
| GET | `/relay` | websocket | live log tail |

`api_version` is `13`. It is bumped when the shape of these responses changes, so
you can rely on the version you see in the samples.

### `/plans` — the payload most of the UI reads

```
catalog             551 rows  {id, name, kind}          troops and tools, for every picker
kingdoms              6 rows  {id, name, constant}
attacks               1 row   saved attack payloads
tasks                 3 rows  saved tasks
task_runtimes         3 rows  per-task commander allocation
subscriptions         3 rows  task -> target filter
modes                 4 rows  saved attack modes
account_modes         1 row   which mode an account is subscribed to
recruitments          0 rows  reusable recruitment objects
recruit_bots          0 rows  castle subscriptions + cadence
account_recruit_bots  0 rows  which recruit bot an account runs
recruit_states        0 rows  per-castle queue estimate
castles               6 rows  the account's own castles
```

Real captured examples of all of these are in `api-samples/`. `plans.json` is
complete and unedited — open it whenever a shape is unclear.

Note that `recruitments`, `recruit_bots` and `recruit_states` are empty in the
sample because none were saved at capture time. Their shapes are still present
and correct (empty arrays), and `src/main.js` already renders them.

---

## Files

```
ui/index.html        all eight views, as sibling <section id="view-..."> blocks
ui/src/main.js       every behaviour and every render function (~1500 lines)
ui/src/style.css     all styling; design tokens are the CSS custom properties at the top
ui/app-icon.svg      the app mark
build/package.json   Vite config, for previewing only
build/vite.config.js
api-samples/*.json   real API responses captured from a running instance
preview/mock-server.py  serves the UI with those samples, so it renders with data
```

### How the UI is put together

- **Views**: eight `<button data-view="x">` in the sidebar switch eight
  `<section id="view-x">` blocks. `switchView(name)` in `main.js` toggles the
  `hidden` attribute and sets the page title. Adding or renaming a view means
  touching that `titles` map too.
- **Icons are text glyphs**, not an icon font or SVG set: `▤ ⌁ ◇ ▦ ＋ ⌘ ● ≡`.
  Swap them freely, but keep them single characters or the nav spacing shifts.
- **Everything is rendered by `main.js`** — the HTML is mostly empty containers
  with ids. The render functions are `renderLibrary`, `renderModes`,
  `renderRecruitmentBuilder`, `renderAccounts`, `renderRun`, `renderDashboard`,
  `renderLogs`. `element(tag, className, text)` is the small DOM helper they all
  use.
- **Polling**: `refresh()` runs every 2 seconds and re-renders from the API. There
  is no client state store — the server is the source of truth. Keep it that way
  or the polling will fight your changes.
- **No external assets**: no images, no fonts, no CDN. The only `url()` in the CSS
  is an inline SVG gradient reference (`#ruby-fill`). Please keep it
  dependency-free so it still builds for a distributable.

---

## Previewing it with data

The UI shows empty states without a server. To see it populated:

```bash
python3 preview/mock-server.py          # then open http://127.0.0.1:47821
```

It serves `ui/` and answers every `/v1/*` call from `api-samples/`, so all eight
views render with the real shapes. Two things worth knowing:

- The page and the API **must share an origin** (the real daemon only allows a
  couple of origins, so a second port is blocked by CORS). The preview handles
  this by serving both from one port, and by pointing the page at whatever port
  it is given — so it also works on a spare port if 47821 is taken:

  ```bash
  python3 preview/mock-server.py 4180   # then open http://127.0.0.1:4180
  ```

- If the port is busy the real app is probably running. Quit it, or use a spare
  port as above.

For the real dev server instead:

```bash
cd build && npm install && npm run dev   # http://localhost:1420
```

---

## Not included, on purpose

- All of `src-tauri/` — the Rust shell, the database, the automation engine.
- The ~50 platform icon variants in `src-tauri/icons/` (Windows/iOS/Android
  launchers). Only `app-icon.svg` is the actual mark.
- `desktop/dist/` — the minified build output, regenerated from these sources.

## Before handing changes back

- `ui/index.html` and `ui/src/` replace `rust_distributable/desktop/index.html`
  and `rust_distributable/desktop/src/` one-for-one.
- Keep every `id` that `main.js` selects on, or update both files together.
- Anything user-visible should read as plain English; the audience is not
  technical, and the existing copy avoids jargon deliberately (for example
  "randomised gaps, like the real client" rather than "bounded Gaussian").
