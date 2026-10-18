# Map, kingdoms and fortresses

How map and castle context works, how large a `gaa` window may be, and how fortresses differ from robber baron
towers. Packet layouts: [`../network/gaa.md`](../network/gaa.md), [`../network/adi.md`](../network/adi.md). Observed in
captures from 2026-10; the evidence is stated with each fact.

## Castle view, map view and kingdoms

Opening the general map is primarily a local client UI transition. No dedicated
"close castle" network command was observed. Once the map was visible, the
client emitted `gbl`, `upt`, and groups of `gaa` reads.

`gaa` supplies the positive network evidence for map context:

- `KID=0` requests and responses were observed for the Green kingdom map.
- `KID=1` requests and responses were observed after moving to Burning Sands.
- The official client commonly requests an inclusive 13-by-13 rectangle
  (`AX1`, `AY1`, `AX2`, `AY2`), with adjacent tiles advancing by 13. That is not
  the server limit: live ladder probes accepted inclusive widths of 23, 62, and
  101 coordinates; a width of 138 was refused with an empty payload.
- The captured Sands viewport began at `572:598` and requested three columns by
  two rows. Those coordinates are account/view dependent and are configurable.

A successful `jaa` is authoritative castle context. A successful `gaa` is
authoritative map context for exactly the returned `KID`; cached map rows prove
what was previously discovered but do not prove where the live session is now.
The safe initial transition begins with Green `jaa`, then obtains a live `gaa`
from each owned permanent kingdom. The observed/client-compatible order used by
OpenAuto is Green (`KID=0`), Ice (`KID=2`), Sands (`KID=1`), then Fire
(`KID=3`), omitting kingdoms absent from `gbd`. Each change is preceded by
`gbl` and `upt`; only the matching live `gaa` advances the state machine. A
network-only client cannot imply that a separate game webview has visually
changed.

The radius is measured from that kingdom's main castle and applies separately
to every owned permanent kingdom. Because the accepted inclusive width is 101,
a radius of 50 fits one `gaa` response exactly. Larger radii are tiled in
101-coordinate windows. Persisted windows can suppress redundant discovery
reads on a normal reconnect, but one live `gaa` per kingdom is still required
as navigation proof.

Recruitment is allowed only after a `jaa` whose castle and kingdom match the
requested subscription. Attack inspection is allowed only after a `gaa` whose
kingdom matches the attack task, and `cra` rechecks that context before commit.
After recruiting in a different kingdom, the attack runner must obtain a fresh
map response for its own kingdom before requesting attack details (`adi` for
RBCs, `abi` for fortresses).

## Fortress rows in `gaa`

Burning Sands fortresses are distinguishable from ordinary RBC towers using
`gaa` alone. An RBC is an `AI` row whose first value is area type `2`. The
captured fortress rows use this separate shape:

`[11, x, y, -1, npc_level, cooldown_seconds, player_id, kingdom_id]`

The evidence is mechanical rather than inferred from the icon name. Captured
type-11 coordinates form the fortress lattice (`575:614`, `594:633`,
`614:614`, `633:633`), and field 5 fell by exactly the elapsed wall-clock time
across repeated `gaa` reads. At `575:614`, a later capture changed field 6 and
reset field 5 from roughly 12.7 hours to 20.4 hours, consistent with another
player defeating it and starting a new cooldown. The user-confirmed fortress at
`594:594` therefore has the same classifier: area type `11`, not type `2`.

An independent `gbl` lookup at `594:633` returned the identical `AI` row with
`N: "Desert fortress"`. That directly confirms the area-type mapping; it is
not merely an inference from the coordinate lattice or cooldown behaviour.

A later Fire Peaks capture independently produced four type-11 rows at
`692:575`, `672:594`, `711:594`, and `731:575`. Each ended in kingdom id `3`
and carried NPC level `55`, rather than the Sands value `45`. Their reported
cooldowns were respectively `47500`, `54673`, `54301`, and `21252` seconds.
This confirms that field 4 is the kingdom-specific NPC level and that type `11`
is the cross-kingdom fortress discriminator.

OpenAuto checks the trailing kingdom id as well as area type `11`, stores these
targets separately from RBCs, and converts the observed remaining seconds into
an absolute availability time. The exact human-facing meaning of field 6 is not
required for classification; it is retained as the raw occupier/player id.

## Fortress dispatch window

The broad scan supplies each fortress's variable remaining cooldown. OpenAuto
converts it to an absolute time and counts down locally; it does not poll every
fortress again before attacking. The target is eligible only from that time
through the following 60 seconds. Once the minute passes it is excluded from
both dispatch and the active dashboard list until a later scan supplies new
server state. This is the same scheduling pattern used for RBCs, whose next
eligibility is tracked from their roughly three-hour cooldown, except fortress
cooldowns are variable and supplied by `gaa`.

An ordinary reconnect reuses the stored map and does not repeat this work. An
explicit user-requested map scan reuses the already learned fortress boundary
but walks its broad windows again, refreshing all cooldowns in batches rather
than polling coordinates one by one.

Captured successful RBC inspections return a populated `adi` response containing
the target, troop inventory, and commander roster. Fortresses are type-11 boss
dungeons and use `abi` for this step; type-2 RBCs use `adi`. The coordinate fields
are the same (`KID`, `SX`, `SY`, `TX`, `TY`), followed by `cra` after a successful
response. Response matching must use the selected command: an unrelated map or
attack-information reply cannot consume the pending request.

Evidence on 2026-10-05: GAA showed Sands fortresses at 516:984 and 575:302 with
zero cooldown; ADI against both immediately returned status 6 and null. This
does not prove a cooldown failure. The client-derived protocol definitions in
[EmpireCore's attack models](https://github.com/eschnitzler/EmpireCore/blob/master/src/empire_core/attack/models/target_info.py)
map boss-dungeon inspection to ABI; its
[error definitions](https://github.com/eschnitzler/EmpireCore/blob/master/src/empire_core/protocol/errors.py)
name status 6 `INVALID_POSITION`. Live ABI acceptance must be checked separately
from that source evidence. Any rejected or null inspection response blocks CRA
and temporarily defers the fortress to avoid a repeated refusal loop.

## Outer-kingdom map boundary and fortress coverage

Direct `gaa` boundary probes returned ordinary map rows at coordinates through
`1285` on both axes. A window at `1286` was partial or empty depending on its
other axis, and windows at `1290` and above returned an empty `AI` list. The
last coordinate on the observed fortress lattice inside that map is `1277`.

The fortress lattice contains two interleaved residue families, so the bounded
rectangle from `0:0` through `1285:1285` contains 2,178 candidate coordinates
per outer kingdom. One accepted 101-coordinate square spans three positions on
each axis in both families—18 candidate coordinates in one response. Captures
also show populated fortress bands separated by empty lattice slots, so a
single empty window does not establish the end of a band. Green has no fortress
type in the captured game data and is excluded from fortress traversal.
