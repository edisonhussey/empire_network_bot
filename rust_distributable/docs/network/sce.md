# `sce`: item stock

| | |
| --- | --- |
| Direction | server to client (push); the full list is inside `gbd` as `sce` |
| Evidence | proven live |
| Code | `empire-core/src/transfer.rs` (`parse_skip_stock`), `empire-daemon/src/direct/transfer.rs` (`observe`) |

## Purpose

Counts of the account's items by key, among them the minute-skip items `MS1`..`MS7`.

## Push

```
%xt%sce%1%0%[["MS5",995]]%
```

A list of `[key, count]`. After each [`msk`](msk.md) it carries the one item just spent (996, then 995, across two skips). `gbd` carries the whole list at login (80 keys on one account, `MS1`..`MS5` and `MS7` among them, `MS6` absent).

## Rules and behaviour

* Set each listed key's count; keys not mentioned keep their value.
* Both captures after a skip list only the item just spent; whether `sce` ever lists others is not known, so an item's stock is unknown until `gbd` or a spend reports it.
* Only the `MS` keys are kept by the code; the others are not interpreted.

## Handling in the code

Stored per account in `app_state` (`transfer.skips.<account>`) and used by the skip-stock check.

## Captures

[`sce_push.txt`](captures/sce_push.txt).

## Open questions

What the other keys (`KT`, `TS`, `PR`, ...) count.
