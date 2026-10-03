# Task 3 — modular attack planning

## Delivered

- A typed Rust planning model for attacks, tasks, and ordered modes. Attacks
  always contain four waves and retain the protocol's left, center, and right
  troop/tool slot shapes.
- A generated catalog of 257 troop types and 294 tool types sourced from the
  existing Python game data. The interface displays a scrollable, searchable
  list of human names and requires an explicit confirmation after quantity is
  selected; protocol IDs remain an internal serialization detail.
- Reusable tasks with a source, dynamic target, attack, target-selection
  algorithm, coin travel, priority, and internal HBW value. Commander capacity
  belongs to a mode allocation rather than the task. The HBW is intentionally not exposed as a
  normal user setting because its stability conditions have not been proven.
- New farming tasks instead select one kingdom and resolve that kingdom's main
  castle at runtime. Their targets are a Robber Baron level band rather than a
  manually entered coordinate; cross-kingdom task definitions are rejected.
- Fortress is persisted as one target type per outer kingdom, without a level
  range, in Sands, Ice, and Fire. It is hidden in Green and is not executable until
  their live network behavior has been verified.
- Human-readable kingdom selectors backed by the generated Rust kingdom
  catalog. Numeric kingdom IDs are retained only in stored/network data.
- Ordered modes with a horizontal, left-to-right scheduler and human-readable
  one-based commander ranges. Reusable tasks remain in the available list;
  adding one again increases its allocation. Add, left, right, count, and remove
  controls make ordering usable even where native drag-and-drop is unreliable. Assigning a
  mode rejects allocations larger than the live account's discovered commander
  count.
- Durable map-discovery health per main castle: learned target count, requested
  map-window count, and last scan time. Recent windows survive restarts, refresh
  after a stable jittered interval around twelve hours, and required reads are
  paced instead of emitted as one burst.
- Sands initialization uses map-coordinate radius: radius 6 fits one 13×13
  `gaa`, radius 50 is an 8×8 grid (64 requests), and the bounded override is
  0–500. The UI shows the grid/request/time estimate. Long scans send periodic
  keepalives. Cached areas are skipped, so a
  larger radius only requests missing or stale coverage.
- A database constraint that allows each account to subscribe to at most one
  mode. The account page assigns that mode and persists its running/stopped
  scheduler state in the detached background service, so minimizing the app
  does not discard it. Attack and task names are unique, and saved attacks, tasks, and modes
  can be copied as JSON or deleted.
- Atomic mode import. Embedded attacks receive new internal IDs, then tasks and
  their order are created in one transaction; invalid or duplicate imports
  leave no partial records.
- A complete Ventrilo Sands example containing the observed Crossbowman and
  Renegade Kunai Thrower formations and a 17/18 commander split.
- A dashboard backed by the real march ledger, live session state, map health,
  and mode assignment. It distinguishes an active attack runner from a merely
  enabled mode or connected account. Its fixed-height activity feed merges
  attacks, completed scan batches, and live scan progress. A lightweight
  24-hour ruby line uses 15-minute database buckets, while attacks, returns,
  rubies, and coins per rolling hour refresh once per minute. The Accounts page also includes a
  copyable live recent-event console that polls while visible, reports its last
  update and newest sequence number, and removes passwords and token fields.

## Python evidence used

The existing Python configuration identifies Crossbowman as item `607`,
Renegade Kunai Thrower as item `35`, and Scaling Ladder as item `614`. It also
provides the proven Ventrilo source coordinate (`593:613`) and the historical
four-wave Kunai and Crossbowman formations. These values are evidence for the
example only; the Rust planner and persistence layer do not execute Python or
PyGGE at runtime.

## Execution boundary

This task delivers creation, validation, persistence, import, export, account
assignment, and durable start/stop intent. A generic runtime that consumes newly authored mode rows and sends
their attacks through the live session scheduler is not yet connected. Existing
network/login and hunt components remain separate until that executor can
preserve the required packet pacing and navigation invariants.

Fortress is similarly configuration-only in this version. The UI labels it as
a placeholder and the executor must not send it as though it were an observed
Robber Baron packet.
