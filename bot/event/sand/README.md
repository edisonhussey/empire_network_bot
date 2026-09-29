# Burning Sands

- `config.py` — attack armies, tools, level ranges, commander counts and priorities.
- `controller.py` — Sands status/control loop.

Run it from the repository root while mitmdump and the game are connected:

```bash
python -m bot.cli --ventrilo proxy start --mode sands --max-attacks 200
```

Use the matching account flag. Account-to-task subscriptions remain in
`bot/tasks.py` because they can combine tasks across modes.
