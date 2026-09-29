# Berimond Kingdom

- `run_config.json` — hands-off run settings used by `python run_berimond.py`.
- `config.py` — source/target defaults, timing, camp capacity and the attack
  composition.
- `refill.py` — camp-stock projection and conservative refill calculations.
- `controller.py` — target discovery, Berimond waits, refill/time-skip flow and
  run supervision.

The mitmproxy addon may stay running. Each command carries a snapshot of the
current attack configuration, so restarting the proxy is not required after
editing `config.py`; start a new controller run to use the edit.

Run it from the repository root after editing `run_config.json`:

```bash
python run_berimond.py
```
