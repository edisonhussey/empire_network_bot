# `cra`: Create Attack

| | |
| --- | --- |
| Direction | client to server, with a reply |
| Evidence | captured, and used live by the bot |
| Code | `empire-daemon/src/direct/automation.rs` (`ReadyCra`, `AwaitCra`), `empire-core/src/hunt.rs`, `empire-game/src/attack.rs` |

## Purpose

Commits a commander, troops and tools to a march against a target.

## Request

```jsonc
{
  "SX": 593, "SY": 613,      // source coordinates
  "TX": 554, "TY": 655,      // target coordinates
  "KID": 1,                  // kingdom id
  "LID": 16,                 // lord id (commander pool id)
  "WT": 0,                   // wave type (0 for standard attacks)
  "HBW": 1009,               // horses / travel mode
  "BPC": 0, "ATT": 0, "AV": 0, "LP": 0, "FC": 0,
  "PTT": 0, "SD": 0, "ICA": 0, "CD": 99,
  "A": [ ... ],              // the attack wave payload (nested arrays)
  "BKS": [], "AST": [-1,-1,-1],
  "RW": [[-1,0], [-1,0], ...],
  "ASCT": 0
}
```

### The wave payload (`A`)

The exact allocation of troops and tools per flank, per wave. The game enforces strict slot counts depending on the target level: typically 2-6-2 distinct troop slots (left, middle, right) and 2-3-2 distinct tool slots. Missing or empty slots must be padded with `[-1, 0]`.

## Reply

The acknowledgement is read for the march id and travel time (`hunt::march_id_from_ack`, `travel_seconds_from_ack`):

```
%xt%cra%1%0%{"AAM":{"M":{"MID":100838963,"TT":774,"TA":[12,557,527]}}}%
```

A successful `cra` also makes the server push a `gam` ([`gam.md`](gam.md)) that confirms the march. (An older note said `cra` had no reply of its own; the bot's code and its live use read `AAM` from this one.)

## Rules and behaviour

1. **Tool-to-troop ratio (`status: 5`).** If tools to troops on a flank exceed the hard limit (for example 6 troops per tool), the server rejects it with `5`. Never retry the same payload.
2. **Lord in use (`status: 256`).** If `LID` is already marching, `256`. Keep an accurate local ledger of busy commanders, skip them and hold them until their `cat` (return) is seen.
3. **Rate limit.** A strict floor of **4.0 seconds** between consecutive `cra` across the transport. Breaking it risks session drops or bans.
4. Other statuses are in [`../architecture/errors.md`](../architecture/errors.md).

## Handling in the code

After `adi`, a paced pause (`Wait::BeforeAttack`), then `cra`, then the ack (`Wait::AfterAck`). See [`../architecture/attack-scheduling.md`](../architecture/attack-scheduling.md) and [`../architecture/timing.md`](../architecture/timing.md).

## Captures

None saved as a file.

## Open questions

The general meaning of `BPC`, `ATT`, `AV`, `LP`, `FC`, `ICA`, `BKS`, `AST`, `ASCT`.
