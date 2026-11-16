# Storm Islands tasks

A **Storm Islands fort** task (Create task, target type *Storm Islands fort*, only in kingdom 4, levels 60-80 by
default) attacks the storm forts around the account's storm castle. Storm forts are not like towers: they appear, are
taken by other players and vanish, so the task works from what was seen *recently* and reads the map *only when it
needs to*. It is a normal attack-bot task otherwise: same attack profiles, commanders, pacing, `adi` then `cra`.

## One step of the attack bot, for a storm task

```text
storm task with a free commander
  │
  ├─ a fort in range, free, seen in the last 2 min, never attacked by this account?
  │     yes → reserve it → open the storm map if needed → adi (still free?) → cra → this account's cooldown on it
  │            └ adi shows it taken, or no answer, or cra refused  → forget it, pick another (no incident)
  │
  └─ none → read ONE map window around the storm castle (if the 5.5-12.5 s scan wait is over
            and no task in the client's current kingdom has a free commander)
            → forts in the window are recorded, forts gone from it are dropped → next step
```

* **Scan on demand.** Nothing is scanned while the task has a fresh fort, while its commanders are all out, or while the
  bot is busy in another kingdom. A pass walks the 13 x 13 windows within 96 tiles of the castle, nearest ring first,
  shuffled inside each ring, then starts again.
* **New, vanished, in range.** `storm_target` holds one row per fort position. Every storm `gaa` (the bot's or the
  browser's) adds or refreshes forts; the bot's own scan window also deletes the forts no longer in it. Only forts with a
  level in the task's range, both cooldown fields 0 and a sighting within 2 minutes are picked; rows unseen for 4 hours are
  forgotten.
* **Per-account cooldown.** A fort this account attacked (or whose `cra` went unanswered: it may have left) is never picked
  again by it.
* **Tolerance.** A fort that is taken or gone between the scan and the attack is routine: the `adi` reply's own map row,
  an unanswered `adi` and any `cra` refusal except a short army (`101`, `313`) forget the fort and move on. They are counted
  (`storm_fort_gone`, `storm_adi_timeout`, `cra_refused_<status>`) but never count toward the safety pause. A short army
  still stops the bot, as for every attack.
* **Batching per kingdom.** Storm picks follow the attack bot's kingdom grouping like every other task, and storm scans
  wait while the client's current kingdom has work, so the bot does not pull the view between kingdoms for a scan.

## Where the code is

| Concern | File |
| --- | --- |
| Rules: level codes, which rows are forts, free or not, scan windows and waits (pure) | `crates/empire-core/src/storm.rs` |
| Forts seen / vanished / reserved / attacked (`V30`) | `crates/empire-core/src/store/storm.rs` |
| The task kind (`Destination::StormLevelRange`, target kind `storm`) | `crates/empire-core/src/planning.rs`, `store/modes.rs`, `empire-daemon/src/plans.rs` |
| Picks, scans (`AwaitStormScan`), tolerance, cooldown on success | `crates/empire-daemon/src/direct/automation.rs` (`target_kind == "storm"`, `StormScan`) |
| Tests: scan then attack, vanished fort, refusals | `crates/empire-daemon/src/direct/storm_tests.rs` |
| UI: target type, kingdom 4 only, default 60-80 | `desktop/index.html`, `desktop/src/main.js` (`updateTargetKinds`) |

Packets: [`gaa`](../network/gaa.md) (storm rows), `adi`, `cra`. The storm castle is the account's kingdom-4 castle
(`owned_castle`, area type 12), learned at login like any other; *use main castle* picks it.

## Earlier version

The Python proxy bot (`bot/event/storm/`) did the same with a continuous scan loop. This version keeps its level codes,
freshness, cooldown memory and refusal tolerance, but scans only when a task needs a fort.
