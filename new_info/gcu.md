# `gcu`: coins and rubies

```
%xt%gcu%1%0%{"C1":2147483647,"C2":150163}%
```

`C1` coins, `C2` rubies, the account's current totals. Pushed unasked now and then, and after anything
that spends them (it followed a `kut` with `C1` 210 lower, the transfer cost, see [`kut.md`](kut.md)).
`2147483647` is the 32-bit maximum, so the coin counter is capped there. Set both numbers from each
`gcu`; a send must not wait for one.
