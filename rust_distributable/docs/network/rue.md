# `rue`: unit count update (recruitment)

| | |
| --- | --- |
| Direction | server to client (push) |
| Evidence | captured (one) |
| Code | not handled yet |

## Purpose

Updates one castle's count for one unit as units are recruited.

## Push

```
%xt%rue%1%0%{"AID":16011862,"SID":0,"WID":217,"NUA":31039,"RUA":5}%
```

| Field | Meaning |
| --- | --- |
| `AID` | the **castle** id (the same `AID` as in [`dcl`](dcl.md)) |
| `WID` | the unit id |
| `NUA` | the new total of that unit in that castle (matches `[217,31039]` in the `dcl` taken right after) |
| `RUA` | the amount recruited |
| `SID` | not known (0) |

## Rules and behaviour

Apply as an absolute value (`NUA`), not as `+RUA`, so a repeat or a reordering does no harm.

## Handling in the code

Not handled: castle stock is refreshed by [`dcl`](dcl.md) instead.

## Captures

[`rue_push.txt`](captures/rue_push.txt).

## Open questions

`SID`; other fields on other recruitment kinds.
