# `gam` (Game Action March)

The `gam` packet is an inbound broadcast from the server. It represents a global update to the player's active marches.

## Inbound Payload

When a `cra` is successfully accepted, the server immediately pushes a `gam` packet containing the official March ID (`MID`) and the exact travel duration (`TT`).

**Example Payload:**
```jsonc
{
  "M": [
    {
      "A": [[216,659041],[227,556966]],
      "M": {
        "D": 0, "HBW": 1009, "KID": 0, 
        "MID": 102744297,                // The unique March ID
        "OID": 12748071, "PT": 0,
        "SA": [ ... ],                   // Source Area details
        "SID": 12748071, 
        "T": 1,                          // March Type
        "TA": [4, 319, 1126, ...],       // Target Area details
        "TT": 1275                       // Travel Time (duration in seconds)
      }
    }
  ]
}
```

### Observed Behaviors & Edge Cases

1. **Travel Duration (`TT`) & Cooldowns**:
   The `TT` field is the absolute source of truth for how long the march will take to land. 
   When calculating target cooldowns (e.g., an RBC's 3-hour cooldown after landing), the bot must extract `TT`, convert it to milliseconds, and set the lower bound directly: `cooldown = moment_sent + TT + 3_hours`. This guarantees perfect cooldown tracking even if the socket disconnects and the landing packet is missed.

2. **March Tracking (`MID`)**:
   The `MID` generated in `gam` is the primary key for the outbound journey. However, **this `MID` is completely destroyed and regenerated when the march lands and begins its return journey**. You cannot join a return packet (`cat`) to an outbound `gam` using `MID`!
