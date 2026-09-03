# Berimond Development Notes

Observed from recent logs for 2026-09-02:

- `bot/game_data/kingdom.py` already maps `BERIMOND_KINGDOM` to ID `10`.
- The active scheduler does not currently include Berimond. It rotates Sands and Storm only.
- Recent Berimond-related data appears in `gaa`, `gam`, and `gdi` packets.
- `gaa` packets observed with top-level `KID = 1` contain area rows where `row[0] == 10`.
- Those `gaa` area rows have the shape:
  `[10, x, y, object_id, owner_id, resource_type, tier_or_size, flag, name]`.
- From named `gaa` rows, resource type appears to be:
  `0 = wood`, `1 = stone`, `2 = food`.
- Player detail packets expose Berimond villages through `O.VP` rows:
  `[resource_or_category, object_id, x, y, 10]`.
- `O.VP` rows use values `1`, `2`, and `3`; because `gaa` uses `0`, `1`, `2`, this should not be treated as the same mapping yet.
- `gdi.gcl.C` can expose the player's own Berimond camp under `KID = 10`.
  Example camp row observed:
  `[15, 1319, 5, 449, 8084471, ..., "GOODY", ..., 10, 0, 0]`.

Observed from account logs for 2026-09-03:

- Manual Berimond `cra` sends were `KID = 10`, source `1310:111`, target `1097:68`.
- The burst at `08:31:00` through `08:31:27` showed accepted LIDs `0, 2, 3, 6, 7, 8, 9`; one `LID 9` send at `08:31:23` returned status `90`, consistent with the 4 second global send limit.
- Berimond `cat` return packets use `KID = 10`, source area `SA = [10, 1097, 68, ...]`, and include loot, for example `coin_loot = 29480`, `ruby_loot = 22`.
- CAT return movement `MID` differs from the original CRA movement, so Berimond result matching should use fixed `KID + target coords + LID`, while commander state keeps a guessed availability from CRA timing and is shortened when CAT arrives.

Runtime scope now implemented:

- `development/berimond.py` parses Berimond villages and camps from recorded logs.
- It includes a `BerimondCycleState` helper for tracking event presence and expected return checks.
- Live runtime defaults and the attack object are defined in `bot/berimond.py`.
- `bot.py --mode berimond-proxy` drives a separate proxy mode through `rbc_proxy_listener.py`.
- Berimond attacks write `target_kind = 'berimond_fixed'`, `kingdom_id = 10`, and do not store raw CAT packet payloads.

Open decision:

- Replace the fixed target coordinates once the target-selection rule is known.
- Confirm whether cycle learning should be based on GAA sightings, CAT returns, or both.
