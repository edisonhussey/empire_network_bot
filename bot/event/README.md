# Event modes

Each game mode owns its controller and editable attack configuration here.
Shared account, database, commander and proxy transport code remains directly
under `bot/`.

| Mode | Edit attacks | Controller | Notes |
| --- | --- | --- | --- |
| Burning Sands | `sand/config.py` | `sand/controller.py` | `sand/README.md` |
| Storm Islands | `storm/config.py` | `storm/controller.py` | `storm/README.md` |
| Berimond Kingdom | `berimond_kingdom/config.py` | `berimond_kingdom/controller.py` | `berimond_kingdom/README.md` |

Files such as `bot/storm.py`, `bot/sands_proxy.py`, and `bot_berimond.py` are
compatibility launchers only. New mode-specific code belongs in this folder.
