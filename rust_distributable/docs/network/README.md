# Empire Network Protocol Reference

This directory contains deep-dive documentation into the raw network packets used by the Goodgame Empire protocol. 

The goal of this folder is to document **observed behaviors**, **payload structures**, and **edge cases** for each packet type. By understanding the raw protocol layer natively, implementing new features or robust handling becomes obvious, avoiding band-aid patch fixes.

## Packet Categories

### Attack Lifecycle
* [`adi`](adi.md) - Attack Details Inspection (Target capability query)
* [`cra`](cra.md) - Create Attack (The actual outbound payload)
* [`gam`](gam.md) - Game Action March (Server confirmation & travel duration)
* [`cat`](cat.md) - Commander Action/Return (March landed or returning)

### Map
* [`gaa`](gaa.md) - Map Viewport Scan

Kingdom/context switching (`gbl`, `upt`, `gbd`) is in [`../architecture/navigation.md`](../architecture/navigation.md).

### General Reference
* `status` codes ([`../architecture/errors.md`](../architecture/errors.md)) - Many server responses contain a `status` field. `0` is success. Others indicate rejection (e.g., `95` target occupied/cooling down, `93` commander traveling, `256` lord in use).

How the bot uses these packets (scheduling, cooldowns, reconnecting) is described in [`../architecture/`](../architecture/README.md), not here.
