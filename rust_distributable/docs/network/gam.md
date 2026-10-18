# `gam`: Game Action March

| | |
| --- | --- |
| Direction | server to client (push) |
| Evidence | captured (several) |
| Code | `empire-core/src/pvp.rs`, `empire-core/src/store/pvp.rs`, `empire-daemon/src/direct/pvp.rs`, `empire-daemon/src/direct/station.rs` |

## Purpose

A global update to the account's active marches, and the movements of others that involve it.

## Push

When a `cra` is accepted the server pushes a `gam` with the official march id (`MID`) and the exact travel time (`TT`).

```jsonc
{
  "M": [
    {
      "A": [[216,659041],[227,556966]],
      "M": {
        "D": 0, "HBW": 1009, "KID": 0,
        "MID": 102744297,                // the unique march id
        "OID": 12748071, "PT": 0,
        "SA": [ ... ],                   // source area details
        "SID": 12748071,
        "T": 1,                          // march type
        "TA": [4, 319, 1126, ...],       // target area details
        "TT": 1275                       // travel time in seconds
      }
    }
  ]
}
```

## Rules and behaviour

1. **`TT` is the source of truth for the journey.** To track a tower's cooldown after landing: `cooldown = moment_sent + TT + 3 hours`. That stays correct even if the socket drops and the landing packet is missed.
2. **`MID` is replaced on landing.** The `MID` here is the key of the outbound journey, but it is destroyed and regenerated when the march lands and starts back. A return (`cat`) cannot be joined to an outbound `gam` by `MID`.
3. **Other players' movements.** The same packet carries movements aimed at the account; how they are read, classified and kept is in [`topics/incoming_movements.md`](topics/incoming_movements.md) and [`topics/alliance_fields.md`](topics/alliance_fields.md).

## Handling in the code

Incoming movements feed the PVP tab and auto-station ([`sequences/support.md`](sequences/support.md)); a retreat arrives as [`mrm`](mrm.md).

## Captures

[`gam_incoming_fixture.txt`](captures/gam_incoming_fixture.txt), [`gam_alliance_target.txt`](captures/gam_alliance_target.txt), [`gam_attack_on_owned_castle.txt`](captures/gam_attack_on_owned_castle.txt).

## Open questions

See [`topics/incoming_movements.md`](topics/incoming_movements.md), "Not established".
