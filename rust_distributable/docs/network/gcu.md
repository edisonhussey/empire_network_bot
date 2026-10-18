# `gcu`: Currency update (coins and rubies)

| | |
| --- | --- |
| Direction | server to client (push); also nested as `gcu` in `gbd`, `kut` and other replies |
| Evidence | proven live |
| Code | `empire-core/src/transfer.rs` (`find_currency`), `empire-daemon/src/direct/transfer.rs` (`observe`) |

## Purpose

The account's current coin and ruby totals.

## Push

```
%xt%gcu%1%0%{"C1":2147480937,"C2":150163}%
```

| Field | Meaning |
| --- | --- |
| `C1` | coins; capped at 2147483647 (the 32-bit maximum) |
| `C2` | rubies |

## Rules and behaviour

* Sporadic, and sent after anything that spends them: a [`kut`](kut.md) was followed by a `gcu` with the cost taken off (2147483437 to 2147480937, exactly 2,500).
* Absolute numbers: set both from each one; a send must not wait for one.

## Handling in the code

Stored per account in `app_state` (`transfer.currency.<account>`) and used by the coin check.

## Captures

[`gcu_push.txt`](captures/gcu_push.txt).

## Open questions

None known.
