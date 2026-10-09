# Server Error Codes (`status`)

Many API requests (like `cra` and `adi`) can be rejected by the server. When this happens, the inbound response will contain a non-zero `status` code.

These are not necessarily "bot failures"—they are usually just the game engine enforcing physical rules (e.g., trying to hit a target that just vanished, or trying to send a commander who is already walking).

| Code | Meaning | Bot Handling Rule |
|---|---|---|
| `0` | Success | Continue normally |
| `5` | Invalid Tool/Troop Ratio | The payload violates game rules (e.g., more than 1 tool per 6 troops). Do not retry without re-computing the payload. |
| `90` | Not Enough Resources | The castle doesn't have the required coins/rubies/troops. Pause operations and refill. |
| `93` | Commander Unavailable | The `LID` is already marching or resting. Wait for the `cat` packet to release them. |
| `95` | Target Occupied / Cooldown | The map entity is cooling down (hit by you or others) and the client's map is stale; the tile's `gaa` row has the real remaining cooldown. For Berimond camps (`KID: 10`) it means the camp is gone. See [`attack-scheduling.md`](attack-scheduling.md) for how the bot reacts. |
| `203` | Target Defeated | Specific to Berimond/Events. Target is completely gone. Retarget. |
| `256` | Lord in Use | Similar to 93. Pick a different commander from the usable pool. |
| `313` | Not Enough Troops | Your castle ran out of units. Halt the mode. |
| `101` | Army Not Available | A `cra` asked for units the castle does not have at home (seen with 50 crossbows requested, 6 to 12 at home). The reply says nothing more. Three in 40 s were followed by a 24 h account lock. Never retry: check `gui.I` from the `adi` reply first. Python saw it only on Berimond and `storm`. |

## Login (`lli`) statuses

| Code | Meaning | Handling |
|---|---|---|
| `27` | Account locked | Payload `{"GDPR":0,"RS":<seconds>}`; `RS` counts down to a fixed expiry (24 h after the trigger). Do not log in again until it passes; every attempt is refused and only adds suspicious traffic. |
