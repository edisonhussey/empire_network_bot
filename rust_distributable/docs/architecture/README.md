# Architecture

How the bot is built and why, split by topic so each part can be read on its own.
Raw packet formats live in [`../network/`](../network/README.md); this folder is
about the system that uses them.

| Read this | When you want to know |
| --- | --- |
| [`decisions.md`](decisions.md) | Why Tauri, and the bounds placed on data and packets |
| [`security.md`](security.md) | The loopback security boundary and the licence trust chain |
| [`runtime-flow.md`](runtime-flow.md) | How the window, daemon and game server fit together |
| [`target-selection.md`](target-selection.md) | Which tower is attacked next: the spotlight algorithm, momentum, tuning and measured trade-offs |
| [`timing.md`](timing.md) | The stochastic wait generator and how global limits override it |
| [`response-health.md`](response-health.md) | Missing and null responses, the rolling-hour tolerance and the safe pause |
| [`development-tab.md`](development-tab.md) | The read-only Development tab: timing graph, kingdom map, movements |
| [`attack-scheduling.md`](attack-scheduling.md) | How towers and commanders are chosen, cooldowns, why throughput is what it is |
| [`connection-supervision.md`](connection-supervision.md) | What happens when the game connection drops |
| [`database.md`](database.md) | What is stored, per account, for how long, and how to query it |
| [`modules.md`](modules.md) | Which crate or file owns which concern |
| [`working-rules.md`](working-rules.md) | How changes are made here |

## Reference

| File | Contents |
| --- | --- |
| [`navigation.md`](navigation.md) | Kingdom and castle context switching (`gbl`, `upt`, `gbd`) |
| [`errors.md`](errors.md) | Server `status` codes and what each means |
| [`network_requests.md`](network_requests.md) | Packet-by-packet notes, each fact tagged with how it was established |
