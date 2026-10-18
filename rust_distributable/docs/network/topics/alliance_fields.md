# Alliance fields used by the PVP tab

Facts read from captured packets while building the PVP tab. Same confidence rules as
[`incoming_gam.md`](incoming_gam.md): observed, not a complete schema.

## Login `gbd`: who the account is

| Path | Example | Meaning |
| --- | --- | --- |
| `gpi.PID` | `16862926` | The account's player id |
| `gpi.PN` | `Ventrilo` | Player name |
| `gal.AID` | `189639` | Alliance id; `gal` is absent for a player without an alliance |
| `gal.N` | `Sons of Guelon` | Alliance name |
| `ain.A.M[]` | | Alliance member records (same shape as `gam` `O[]`), including `AP` castle lists |

## `gam` player records, `O[]`

Matched to a movement by `OID` (never by position): `N` name, `AID` alliance id (`-1` when none),
`AN` alliance name (only present alongside a non-negative `AID`).

## `gam` army

Other players' movements carry `GA` = `{"L": [...], "M": [...], "R": [...], "RW": [...]}`, each a
list of `[unit_id, count]`. The army size is the sum of the counts; the capture in
[`gam.txt`](gam.txt) has `M: [[602, 9]]`, so 9. Some packets carry only `GS` (see
`incoming_gam.md`); that is not summed, and the army is unknown until a `GA` form arrives.

## `mrm`

`{"MID": <movement id>}`. Treated as a retreat only when received from the server with status
`0`. See [`mrm.md`](mrm.md) for what is and is not established.
