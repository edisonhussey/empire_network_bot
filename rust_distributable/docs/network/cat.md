# `cat` (Commander Action / Target Return)

The `cat` packet is an inbound broadcast from the server indicating that a march has landed, looted, and/or begun its return journey.

## Inbound Payload

When a march hits an RBC, Fortress, or player, the `cat` packet pushes the battle outcome, loot payload, and the commander's return time.

**Example Payload:**
```jsonc
{
  "M": [
    {
      "MID": 101093730,     // A NEW March ID (not the outbound ID)
      "KID": 1, 
      "LID": 16,            // The Commander ID that was used
      "SA": [ ... ],        // Source Area (Now the RBC)
      "TA": [ ... ],        // Target Area (Now your Castle)
      "TT": 1275,           // Return Travel Time
      "LOOT": [ ... ]
    }
  ]
}
```

### Observed Behaviors & Edge Cases

1. **March ID Rotation (`MID`)**:
   As noted, the `MID` here is brand new. To attribute a `cat` to the original `cra`, you must match the packet using the `LID` (Lord ID) and the coordinates. 
   *Note:* The areas swap on the return leg! The RBC is now the Source Area (`SA`), and your castle is the Target Area (`TA`).

2. **Refining Cooldowns**:
   While the outbound `gam` packet provides a highly accurate estimate of the landing time, the arrival of the `cat` packet provides the exact, irrefutable server timestamp of the landing. The bot should use this moment to apply minor heuristic refinements (e.g., +/- random backoff buffers) to the database cooldown timer.

3. **Commander Release**:
   The `cat` packet contains the `TT` (Travel Time) for the return leg. The commander (`LID`) must remain locked in the bot's usable pool until `now + TT + hold_buffer` has elapsed.
