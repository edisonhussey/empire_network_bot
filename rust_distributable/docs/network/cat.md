# `cat`: Commander Action / target return

| | |
| --- | --- |
| Direction | server to client (push) |
| Evidence | captured |
| Code | `empire-core/src/hunt.rs` (`return_target`, `loot_from_return`), `store/ledger.rs` (`apply_attack_return`) |

## Purpose

Says a march has landed, looted and begun its return, with the loot and the return time.

## Push

```jsonc
{
  "M": [
    {
      "MID": 101093730,     // a NEW march id (not the outbound one)
      "KID": 1,
      "LID": 16,            // the commander used
      "SA": [ ... ],        // source area (now the tower)
      "TA": [ ... ],        // target area (now your castle)
      "TT": 1275,           // return travel time
      "LOOT": [ ... ]
    }
  ]
}
```

The loot and result are read by the bot from `A.G` (`[["C1",coins],["C2",rubies]]`) and `A.S` (result flag); the movement from `A.M` (`SA`, `KID`, `MID`, `TT`, `PT`).

## Rules and behaviour

1. **The march id is replaced.** `MID` is new. To attribute a `cat` to the original `cra`, match on `LID` plus the coordinates. The areas **swap** on the return leg: the tower is now `SA`, your castle `TA`.
2. **It refines cooldowns.** The outbound `gam` gives an accurate landing estimate; `cat` is the exact, authoritative landing moment. Use it to apply small refinements (for example random back-off) to the stored cooldown.
3. **It releases the commander.** `TT` is the return leg. Keep the commander (`LID`) locked until `now + TT + hold_buffer`.
4. **A looted return is a won attack**, which raises a robber baron's victory count by one ([`../game_infrastructure/robber_baron_level_formula.md`](../game_infrastructure/robber_baron_level_formula.md)).

## Handling in the code

`apply_attack_return` matches it to the in-flight march by target and commander, closes the ledger row and, for a tower with loot, records the victory.

## Captures

None saved as a file.

## Open questions

The full meaning of `A.S` values.
