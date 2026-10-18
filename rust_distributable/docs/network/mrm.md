# `mrm`: Movement Retreat

| | |
| --- | --- |
| Direction | server to client (observed); the same shape is the retreat command |
| Evidence | captured (one example) |
| Code | `empire-core/src/pvp.rs`, `empire-daemon/src/direct/pvp.rs`, `empire-daemon/src/direct/station.rs` |

## Purpose

An attack was retreated.

## Push

```text
%xt%mrm%1%0%{"MID":155312686}%
```

| Part | Example | Meaning |
| --- | --- | --- |
| XT command | `mrm` | movement retreat |
| JSON `MID` | `155312686` | the movement id of the attack being retreated; the `M.MID` of a [`gam`](gam.md) record |

## Rules and behaviour

* A retreat **cannot be undone**: that `MID` is finished permanently. Treat it as terminal after a confirmed retreat and never resume or reuse it; a new attack has a new `MID`.
* The single example does not establish whether this exact frame is a client request, a server response or both, nor whether a separate acknowledgement confirms success. Distinguish a sent command from a confirmed retreat before marking an attack terminal. The remaining XT header fields are not interpreted.

## Handling in the code

An inbound successful `mrm` moves a known entry to Retreated ([`topics/incoming_movements.md`](topics/incoming_movements.md)) and releases a waiting support recall ([`sequences/support.md`](sequences/support.md)).

## Captures

[`mrm_retreat.txt`](captures/mrm_retreat.txt).

## Open questions

Direction, acknowledgement and header fields, as above.
