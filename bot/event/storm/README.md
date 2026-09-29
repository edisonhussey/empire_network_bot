# Storm Islands

- `config.py` — Storm army composition, its `storm` task and scan defaults.
  Source coordinates are not configuration: the listener learns the logged-in
  account's owned Storm castle (`KID=4`) from the `gbd` login response.
- `controller.py` — Storm scanning, level gates, randomized waits and attack
  sequencing.
- `database.py` — Storm target parsing, reservation and persistence.

The controller deliberately keeps Storm-specific timing and level detection;
it calls `bot.bot` only for shared account, commander, safety and proxy-message
operations.

Run it from the repository root while mitmdump and the game are connected:

```bash
python -m bot.cli --ventrilo proxy start --mode storm --max-attacks 200
```

Keep `mitmdump -s bot/rbc_proxy_listener.py` running before login. On login the
listener persists that account's current Storm castle and the controller reads
it automatically. If no `KID=4` castle has been learned, startup stops safely
instead of sending from a default coordinate. For protocol debugging only, a
paired override is available:

```bash
python -m bot.cli --ventrilo proxy start --mode storm --max-attacks 1 \
  --source-x 521 --source-y 591
```
