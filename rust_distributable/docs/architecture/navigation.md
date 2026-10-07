# Navigation Packets (`gbl`, `upt`, `gbd`)

Goodgame Empire does not have a dedicated "switch kingdom" packet. Movement between maps and castles is handled via context reloading.

## `gbl`
Sent with an empty payload `{}`. This packet flushes the client's current map context and prepares for a reload.

## `upt`
Sent immediately after `gbl` with an empty payload `{}`. Requests a map context update.

## `gbd`
The bootstrap packet. Carries the initial join payload.
Example: `{"CID":-1,"KID":0}`

## Mode Switching Sequence

**Observed sequence when entering Sands (`KID: 1`):**
```
12:34:16.360 C>S gbl  {}
12:34:16.416 C>S gaa  {"KID":1,"AX1":572,"AY1":598,"AX2":584,"AY2":610}
12:34:16.446 C>S gaa  {"KID":1,"AX1":585,...}   
```
Notice that the kingdom is selected entirely by the `KID` field inside the subsequent `gaa` tile requests. There is no explicit "Move to Sands" packet.

### Castle Switch Floor
When entering a castle to switch context (e.g., for recruitment), the server enforces a strict floor of `≥ 3.0s`. The map viewport (`gaa`) must be explicitly re-requested after leaving the castle mode to resume attacking.
