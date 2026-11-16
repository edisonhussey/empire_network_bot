# Resource bot: the refinery

The resource bot runs an account's resource jobs. Today it does one: it crafts refined wood and/or refined stone
at the refinery of the **Green main castle** (kingdom 0, area type 1; no other castle has it). Moving and transferring
resources will be added to the same bot later, and the toolsmith will be a second building with its own queue and recipes.

## The objects (who picks what)

```text
Resources tab
├── Create refined resource  -> create_resource_preset   (building "refinery", recipes: wood, stone or both)
└── Create resource bot      -> resource_bot             (name + one create-resource preset; later: transfers)
Control Panel, step 2
└── Resource bot: none | <bot>  -> account_resource_bot  (running, started and stopped with the other bots)
```

The same shape as the support bot: the tab only stores, the Control Panel chooses, the session loop acts.

## One visit

```text
            ┌────────────────────────── schedule future (refinery_state.next_due_ms) ◀──────────────┐
            ▼                                                                                        │
 due? ──no──▶ wait (re-checked each minute: a stop or a new preset is seen)                           │
  │yes                                                                                               │
  ▼                                                                                                  │
 jca: open the Green main castle (from wherever the view is, as recruiting does)                     │
  ▼                                                                                                  │
 crin: the refinery (OID, what is producing and waiting) ── free places = 2 − producing − waiting     │
  ▼                                                                                                  │
 dcl: the castle's stock                                                                             │
  ▼                                                                                                  │
 resource check: plan the crafts over the free places, cycling the ticked recipes,                   │
 keeping each only if the stock still covers it after the ones before it ──none──▶ gaa ──────────────┤
  │ some                                                    (log: checking inventory … -> not enough)│
  ▼  (log: checking inventory for castle NAME (ID) -> (wood N, stone M; refinery F of 2 free) -> executing …)
 random stagger 1.5–9 s ─▶ crst (craft 1) ─▶ reply ─▶ random stagger ─▶ crst (craft 2) ─▶ reply       │
  ▼                                                                                                  │
 gaa: back to the map so the attack bot finds the view it expects ───────────────────────────────────┘
```

* **Inventory first, always.** Nothing is sent that the castle cannot pay for, so a sequence never fails half way. With
  wood + stone ticked and wood short, only the stone craft goes; with one recipe ticked, two lots of it if both are
  covered, else one.
* **Alternation survives.** `cycle_index` moves on by the crafts started, so wood and stone take turns being first
  (and so being crafted first when only one place is free).
* **When next.** After a visit: when the refinery will be empty again (`PS.RCT` of the producing craft plus one hour
  per waiting craft), never sooner than **2 hours**, plus 5 to 120 s of jitter. Nothing affordable and the refinery
  idle: 30 minutes. A refused or unanswered step: 30 minutes, with a `refinery.refused` / `refinery.timeout` event.
* **Not at the same second.** Each `crst` waits a random 1.5 to 9 s (`empire_core::pacing::Rng`, the same generator
  every other wait in the project uses).
* **Persisted.** `refinery_state` (one row per account) holds the next visit, the cycle position and what the last
  visit did. A restart, a dropped connection or a new window reads it and does not visit early.

## Sharing the connection

The refinery is a peer of the recruit bot in the session loop (`direct/mod.rs`, automation tick):

* It starts a visit only when the station gate is open and neither the attack bot nor recruitment holds the
  connection. Once started it holds it (`holds_transport`) until it is back on the map, so the attack bot,
  transfers and the Overview's `dcl` wait.
* Recruitment and the refinery never interleave: each yields to the other while it holds the view.
* The dashboard shows `refining` with the step, like `recruiting`.

## Where the code is

| Concern | File |
| --- | --- |
| Rules: recipes, costs, plan, affordability, next visit, stagger (pure, unit tested) | `crates/empire-core/src/refinery.rs` |
| Storage: presets, bots, account choice, schedule (`V29`) | `crates/empire-core/src/store/resource.rs` |
| The visit, packet by packet (state machine, logs) | `crates/empire-daemon/src/direct/refinery.rs` |
| HTTP: `GET /v1/resources`, `POST/DELETE /v1/resources/create-presets[/{id}]`, `POST/DELETE /v1/resources/bots[/{id}]`; the choice is `resource_bot_id` on `POST /v1/plans/start` | `crates/empire-daemon/src/resource.rs`, `plans.rs` |
| UI: Resources tab and its pure helpers; the Control Panel select | `desktop/src/views/resources.js`, `resources-model.js`, `main.js` |

Packets: [`crst`](../network/crst.md), [`crin`](../network/crin.md), [`dcl`](../network/dcl.md).

## Adding the next job (horizontal growth)

* **Toolsmith:** a second building value in `create_resource_preset.building`, its recipes in `refinery.rs`'s sibling
  (its own queue id from `crin`, `CQID` 2 in the capture), and a second visit kind in the daemon. The preset table and
  the bot do not change.
* **Resource transfers:** a new column or child table on `resource_bot` and its own state machine under `direct/`,
  yielding the connection the same way.
