# Attack travel options (`HBW`, `PTT`) and stable levels

How the `HBW` field of [`cra`](../network/cra.md) maps to coin, ruby and feather travel, and why it is per castle.
Observed in Ventrilo and Pingpoko captures. Code: `empire-core/src/store.rs` (castle travel options),
`empire-core/src/planning.rs` (`TravelMode`).

The `HBW` field in `cra` is not one universal coin-horse identifier. Captured
Ventrilo packets from a level-3 stable established this sequence:

- no horse: `HBW=-1`, `PTT=0`
- coin horse: `HBW=1007`, `PTT=0`
- first ruby horse: `HBW=1008`, `PTT=0`
- second ruby horse: `HBW=1009`, `PTT=0`
- feather travel: `HBW=-1`, `PTT=1`

The identifiers form three observed stable families: level 1 uses
`1001–1003`, level 2 uses `1004–1006`, and level 3 uses `1007–1009`. This is a
property of the source castle, not of the account or kingdom. In particular,
Pingpoko's Sands castle advertised `[1004,1005,1006]`; a `cra` sent from it with
the previously hard-coded `1007` was rejected, while the real client used
`1004` successfully.

The account bootstrap exposes the authoritative per-castle list at
`gbd.gpc.A[]`: `AID` is the castle id, `KID` is its kingdom, and `UH` is its
unlocked HBW list. The list order is not the travel-mode order (a captured
level-3 castle returned `[1008,1009,1007]`). Across the observed families the
coin choice is the lowest positive `UH` value. OpenAuto therefore stores this
data for each castle and resolves coin `HBW` from the attack's exact source
castle immediately before constructing `cra`.

Tasks store a human travel mode rather than a castle-specific number. After
sorting the source castle's three positive UH values, OpenAuto maps them as
coin = UH[0], ruby slow = UH[1], and ruby fast = UH[2]. Feather is the separate wire
mode HBW=-1, PTT=1. Coin and both ruby choices use PTT=0. This mapping is
performed at send time so a task can retain the same semantic choice when used
with castles at different stable levels. If no initialized castle matches the
source kingdom and coordinates, no cra is sent; guessing a global HBW is known
to cause server rejection.
