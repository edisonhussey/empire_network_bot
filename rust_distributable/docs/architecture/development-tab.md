# Development tab

A read-only window onto the scheduler, in the app under Diagnostics. It observes; it
cannot start, stop or change the bot, and it never holds any state the bot depends on.
Closing it, or leaving the tab, changes nothing about execution.

## What it shows

* **Human-paced waits**: a log-scale chart (0.5 s to 120 s). The green curve is the wave sum
  drawn continuously (the same waves as the generator, evaluated between whole steps as well
  as at them, no noise). Green points are the last 40 waits actually used, placed by their own
  step number; a hollow ring marks one a global limit made longer. Hollow points to the right of
  the white "now" line are the **projection** of the next 30 waits. The window slides with time:
  "now" stays put and the curve flows through it, moving through each wait as it elapses, so
  nothing jumps when an action happens. The green band is 1 to 5 s, where about 60 % of waits
  are meant to fall, and a dashed line marks the floor `m`. The generator advances one step per
  action (`t = t + 1`), so the chart's unit is the action, not the second.
  A slim **vertical slider** to its right is the test pace (below). Beside it: the wave sum at the next step, the floor, how many waits have been
  generated, the last generated wait and what it was made of (wave, noise, protocol
  minimum), the wait actually applied with any extra the limits added, the share of recent
  waits inside 1 to 5 s, and a live countdown to the next permitted action.
* **Frequency of the waits used**: a histogram of every human-paced wait either generator
  produced (24 log-spaced bins from 0.25 s to 240 s and an overflow bin), the bars inside
  1 to 5 s in green and the share printed. The attack sequence and the station both feed it.
* **Station**: the stationed castles, each with its state, the attackers, and the time
  before the landing at every step (`dcl`, `sdi`, `cds` sent and acknowledged), plus the
  gate and the delay the deadlines allow for.
* **Diagnostics**: scheduler state and detail, the global restriction in force, the last
  completed attack, the current kingdom, eligible towers, how many are near the
  spotlight (and whether it will relocate), and the recent waits.
* **Maps**: one per signed-in account on the viewed account's world, each in its own colour, with the others' movements drawn over it (see [`accounts.md`](accounts.md)). Each map: a 100 x 100 logical window centred on the main castle. White circles are
  towers (hollow while cooling), the square is the main castle, the dashed ring is the
  spotlight with its local radius, and the arrow is its direction. Zoom buttons, wheel
  zoom, drag to pan, "Castle", "Spotlight" and "Reset 100 x 100". One scale serves both
  axes, so the map is never stretched. Only towers inside the window are drawn.
* **Movements**: each army out or coming back is a path between the castle and its
  tower with a marker, a direction arrow and the remaining time. The line ahead of the
  marker stays and the line behind it disappears. When a return time is not known it is
  drawn dashed and labelled "return unknown"; it is never estimated.
* **Support on the map**: the attacked castle is ringed; a dashed line runs to its
  destination, moving the way the troops move; a marker shows the support out or back and
  the labels count down to the recall and to the landing. Only for the kingdom shown.
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

## The test pace

The slider scales every human-paced wait the attack scheduler generates from now on, and the
projection with it: 1 is the design (the default, and what a new session starts at), up is longer
waits, down shorter. It is bounded, 0.5 to 4 (`PACE_MIN`, `PACE_MAX` in `empire-core/src/timing.rs`),
on a log scale so 0.5 and 2 are equally far from 1; double-click or Home returns to 1. It is for
testing: held in memory per session (it survives a reconnect, not a restart), never saved, and
never applied to a wait already running, to the station's deadline-driven waits, or to the
protocol minimums and the 4 s global `cra` spacing, which still hold at any pace.

## Endpoints

| Endpoint | Returns |
| --- | --- |
| `GET /v1/dev` | the snapshot: timing (wave parameters, history, recent waits), scheduler, spatial (castle, spotlight, velocity, last selection) and health |
| `POST /v1/dev/pace` | `{account_id, pace}`: the test pace of that account's waits; answers with what was kept |
| `GET /v1/dev/map?kingdom=` | the kingdom's towers with the time each is next ready, and the main castle |
| `GET /v1/dev/movements?kingdom=` | armies out or returning, with start and end timestamps |
| `GET /v1/pvp?state=active&kingdom=` | other players' travelling movements, drawn as arrows (see [`pvp.md`](pvp.md)) |

None of them carries credentials, tokens or packet payloads.

## Kingdom chooser and PVP arrows

The map's drop-down picks the kingdom shown: "Follow the bot" (the default) or a specific one.
Bold green arrows are alliance attacks going out, bold red arrows attacks coming in to the
alliance (the account included). Sender and target get a dot each, labelled `name | alliance`,
and exist only while the movement is active. Details and limits: [`pvp.md`](pvp.md).

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
