# Server Error Codes (`status`)

Many API requests (like `cra` and `adi`) can be rejected by the server. When this happens, the inbound response will contain a non-zero `status` code.

These are not necessarily "bot failures"—they are usually just the game engine enforcing physical rules (e.g., trying to hit a target that just vanished, or trying to send a commander who is already walking).

| Code | Meaning | Bot Handling Rule |
|---|---|---|
| `0` | Success | Continue normally |
| `5` | Invalid Tool/Troop Ratio | The payload violates game rules (e.g., more than 1 tool per 6 troops). Do not retry without re-computing the payload. |
| `90` | Not Enough Resources | The castle doesn't have the required coins/rubies/troops. Pause operations and refill. |
| `93` | Commander Unavailable | The `LID` is already marching or resting. Wait for the `cat` packet to release them. |
| `95` | Target Occupied / Cooldown | The map entity is cooling down (e.g. RBC hit by you/others) or is quarantined. For normal RBCs, defer target by 1hr. For Berimond camps (`KID: 10`), delete target. |
| `203` | Target Defeated | Specific to Berimond/Events. Target is completely gone. Retarget. |
| `256` | Lord in Use | Similar to 93. Pick a different commander from the usable pool. |
| `313` | Not Enough Troops | Your castle ran out of units. Halt the mode. |
