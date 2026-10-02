# Burning Sands

- `config.py` — attack armies, tools, level ranges, commander counts and priorities.
- `controller.py` — Sands status/control loop.

Run it from the repository root while mitmdump and the game are connected:

```bash
python -m bot.cli --ventrilo proxy start --mode sands --max-attacks 200
```

Recruitment uses the account plan's default. Override it for one run with
`--recruit true` or `--recruit false`; the same option is supported by the
shared Storm and Berimond proxy paths.

Use the matching account flag. Account-to-task subscriptions remain in
`bot/tasks.py` because they can combine tasks across modes.

Successful attack pacing is defined centrally in `bot/pacing.py`. Sands uses a
short randomized delay after a CRA acknowledgement and a separate randomized
ADI-to-CRA deadline. Every mode also passes through the same final, persisted
four-second CRA governor. The older 20–30 second request interval is retained as
error backoff and is not part of a successful Sands attack cycle.

## One-off live packet tests

The running listener accepts test packets from a separate Python process, so
editing or reloading the addon is unnecessary:

```python
from bot.proxy_inject import inject

# Build an XT packet from a command and payload.
result = inject("gaa", {"KID": 1, "AX1": 590, "AY1": 610, "AX2": 602, "AY2": 622})

# Or submit an already captured raw client packet.
result = inject("%xt%EmpireEx_21%gpa%1%{\"AID\":16366514}%")
print(result)
```

With no `account=` argument the function uses the listener's detected login
session. A result of `sent` means the packet reached mitmproxy; the server reply
continues to appear in the normal account capture. Attack commands still require
persisted map mode. Custom CRA packets are refused while automation owns the
transport and always respect the global four-second CRA interval.
