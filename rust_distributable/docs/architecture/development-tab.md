# Development tab

A read-only window onto the scheduler, in the app under Diagnostics. It observes; it
cannot start, stop or change the bot, and it never holds any state the bot depends on.
Closing it, or leaving the tab, changes nothing about execution.

## What it shows

* **Human-paced waits**: a log-scale chart (0.5 s to 120 s). Green points are the last 40
  waits actually used, in order; a hollow ring marks one a global limit made longer. The
  green band is 1 to 5 s, where about 60 % of waits are meant to fall, and a dashed line
  marks the floor `m`. To the right, hollow points are the **projection** of the next 30
  waits from the generator as it stands, labelled a projection. The line is not a clock
  trace: the generator advances one step per action (`t = t + 1`), so the chart is in
  actions. Beside it: the wave sum at the next step, the floor, how many waits have been
  generated, the last generated wait and what it was made of (wave, noise, protocol
  minimum), the wait actually applied with any extra the limits added, the share of recent
  waits inside 1 to 5 s, and a live countdown to the next permitted action.
* **Diagnostics**: scheduler state and detail, the global restriction in force, the last
  completed attack, the current kingdom, eligible towers, how many are near the
  spotlight (and whether it will relocate), and the recent waits.
* **Map**: a 100 x 100 logical window centred on the main castle. White circles are
  towers (hollow while cooling), the square is the main castle, the dashed ring is the
  spotlight with its local radius, and the arrow is its direction. Zoom buttons, wheel
  zoom, drag to pan, "Castle", "Spotlight" and "Reset 100 x 100". One scale serves both
  axes, so the map is never stretched. Only towers inside the window are drawn.
* **Movements**: each army out or coming back is a path between the castle and its
  tower with a marker, a direction arrow and the remaining time. The line ahead of the
  marker stays and the line behind it disappears. When a return time is not known it is
  drawn dashed and labelled "return unknown"; it is never estimated.
* **Response health**: the rolling-hour count of unexpected missing or null responses
  against the tolerance, the state, and the recent incidents.

## How it stays cheap

* The backend copies its state into one snapshot about once a second (memory only, no
  database, no network) and the tab reads it with a plain `GET /v1/dev`.
* The tab polls slowly (snapshot every second, movements every two, tower positions when
  it opens, when the kingdom changes, and every thirty seconds for cooldowns) and does
  all animation itself. each march is
  placed from its start and end timestamps with
  `p = clamp((now - start) / (end - start), 0, 1)`, on every animation frame. There is
  no backend timer per movement, tower or frame.
* While the tab is hidden, every timer and animation frame is stopped. The backend's
  scheduling and health monitoring are not.
* History is bounded: 300 signal points, 24 recent waits, 64 incidents.

## Endpoints

| Endpoint | Returns |
| --- | --- |
| `GET /v1/dev` | the snapshot: timing (wave parameters, history, recent waits), scheduler, spatial (castle, spotlight, velocity, last selection) and health |
| `GET /v1/dev/map?kingdom=` | the kingdom's towers with the time each is next ready, and the main castle |
| `GET /v1/dev/movements?kingdom=` | armies out or returning, with start and end timestamps |

None of them carries credentials, tokens or packet payloads.

## Movements, from the ledger

* **Outbound**: from the attack's send time for the travel time its acknowledgement gave.
  Once that time has passed without a report it is shown as landed with the return
  unknown.
* **Returning**: from the landing report for the return time that report carried.
* Rows older than four hours are ignored, so a report that never arrived does not draw a
  path forever.

## Tests

`npm test` in `desktop/` checks the pure geometry (`dev-math.js`): the wave sum, the
interval, progress and clamping, unknown timestamps, path endpoints for outbound and
return, viewport proportions on any canvas shape, which objects are visible, anchored zoom
with clamping, and panning. On the Rust side: the movement classification and progress,
the map queries, and a test that taking a snapshot changes nothing in the bot.
