# `sdi`: Support Defence Info (opens the support page)

| | |
| --- | --- |
| Direction | client to server, with a reply |
| Evidence | captured |
| Code | `empire-core/src/station.rs` (`parse_sdi`), `empire-daemon/src/direct/station.rs` |

## Purpose

Opens and inspects the support page for a destination. It does **not** dispatch troops; [`cds`](cds.md) does.

## Request

```
%xt%EmpireEx_49%sdi%1%{"TX":468,"TY":507,"SX":543,"SY":520}%
```

`S` coordinates are the source castle, `T` the destination.

## Reply

| Field | Meaning |
| --- | --- |
| `SCID` | the source castle id; validate it against the selected source castle |
| `gui.I` | `[item_id, count]` rows at home (observed) |
| `gui.TU` | units already out |
| `gli.C` | the commander roster (not proof of a free `LID`) |

A shortened excerpt: `%xt%sdi%1%0%{"SCID":437391,"gaa":{"AI":[1,468,507,408808,561635,...]},"B":{...},"gui":{"I":[[602,9],[637,0],[614,5],[660,3]],"SHI":[],"HI":[],"TU":[]},"gli":{...},...}%` (full text in the capture).

## Rules and behaviour

The support proof of concept intersects positive `gui.I` quantities with a recent matching `dcl` area's `AC` and known troop ids: a conservative way to choose home units. Confirm the exact source and inventory semantics with more captures.

## Handling in the code

Step 2 of [`sequences/support.md`](sequences/support.md).

## Captures

[`sdi_request.txt`](captures/sdi_request.txt), [`sdi_response.txt`](captures/sdi_response.txt).

## Open questions

Refusal statuses (none captured).
