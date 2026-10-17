# `sce`: skip item stock

```
%xt%sce%1%0%[["MS5",998]]%
```

A list of `[skip item key, count left]`. It arrives right after each `msk` and shows the account's stock
of the item just spent (998 then 997 across two skips). Keys are the `MS1`..`MS7` names in
[`msk_info.md`](msk_info.md). Both captures list only the item just spent; whether `sce` ever lists others is not
known, so the stock of an item not yet spent is not known from `sce` alone. Apply it as "set this item's count"; it needs
nothing to wait for it and nothing waits for it.
