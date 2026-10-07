# `cra` (Create Attack)

The `cra` packet is the outbound envelope that actually commits troops and tools to a march against a target.

## Outbound Request

The request defines the commander, the target, travel mode, and the exact wave payload of units.

**Example Payload Structure:**
```jsonc
{
  "SX": 593, "SY": 613,      // Source coordinates
  "TX": 554, "TY": 655,      // Target coordinates
  "KID": 1,                  // Kingdom ID
  "LID": 16,                 // Lord ID (Commander pool ID)
  "WT": 0,                   // Wave Type (0 for standard attacks)
  "HBW": 1009,               // Horses/Travel mode
  "BPC": 0, "ATT": 0, "AV": 0, "LP": 0, "FC": 0,
  "PTT": 0, "SD": 0, "ICA": 0, "CD": 99, 
  "A": [ ... ],              // The Attack Wave Payload (nested arrays)
  "BKS": [], "AST": [-1,-1,-1], 
  "RW": [[-1,0], [-1,0], ...], 
  "ASCT": 0
}
```

### The Wave Payload (`A`)
The `A` array contains the exact allocation of troops and tools per flank, per wave.
* The game enforces strict slot counts depending on the target level.
* Typically: `2-6-2` distinct troop slots (Left, Mid, Right) and `2-3-2` distinct tool slots.
* Missing or empty slots MUST be padded with `[-1, 0]`.

## Inbound Response

The server does not respond to `cra` with its own dedicated wrapper. Instead, a successful `cra` triggers a `gam` (Game Action March) packet from the server broadcast layer, which confirms the march creation.

### Observed Behaviors & Edge Cases

1. **Tool-to-Troop Ratio (`status: 5`)**:
   If the ratio of tools to troops on a flank exceeds the hard limit (e.g., 6 troops per tool), the server rejects the `cra` with `status: 5`. The bot must not retry the same payload.
   
2. **Lord in Use (`status: 256`)**:
   If `LID: 16` is already marching, the server rejects the packet with `256`. The bot must maintain an accurate local ledger of busy commanders to avoid this, skipping busy `LID`s and holding them until their respective `cat` (return) packets are observed.

3. **Rate Limiting**:
   The server enforces a strict floor of **4.0 seconds** between consecutive `cra` packets across the transport layer. Violating this risks immediate session drops or bans.
