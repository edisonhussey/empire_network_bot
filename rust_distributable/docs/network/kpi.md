# `kpi` (Kingdom Info) and the state packets next to it

`kpi` is the server's whole view of the kingdoms: which are unlocked, and what troops or goods are travelling
between them. It is read by [`kut`](kut.md) and [`msk`](msk.md) replies, comes inside `gbd` at login, and is
pushed again when a transfer lands.

```
%xt%EmpireEx_21%kpi%1%{}%
%xt%kpi%1%0%{"UL":[{"KID":0,"U":1,"C":0},{"KID":3,"U":1,"C":1,"SL":1,"SPW":81000,"SPS":81000,"SPF":0,"SPC1":120000},{"KID":4,"KRS":1280228,"CRS":2,"U":0,"C":0}],
              "UT":[{"KID":3,"RS":7200,"I":[[655,20]]}]}%
```

## `UL[]`: one row per kingdom (client `CastleKingdomVO.parseUnlockInfo`)

| Field | Meaning |
| --- | --- |
| `KID` | kingdom id: 0 green, 1 Burning Sands, 2 Everwinter, 3 Fire Peaks, 4 Storm Islands |
| `U` | `1` = unlocked for this account |
| `C` | has a Kontor (the kingdom's trading post); green shows `0` |
| `SL` | slum level (up to 4) |
| `SPW SPS SPF SPC1` | goods already paid toward the next slum level |
| `KRS`, `CRS` | seconds until the kingdom resets and its reward set (kingdom 4) |

## `UT[]` and `RT[]`: what is travelling

`UT`: troops, `{"KID": target, "RS": seconds left, "I": [[unit, count], ...]}`. `RT`: goods, `G` instead of `I`.
The client replaces its list on **every** `kpi`, so one without `UT` means nothing is travelling: treat it as
a snapshot, never a delta. `RS` counts from the packet's arrival.

## Next to it

* `gcu`: `{"C1": coins, "C2": rubies}`; also nested as `gcu` inside `gbd` and other replies. Coins are capped
  at 2147483647. A `kut` is followed by a `gcu` with the cost taken off.
* `sce`: `[[key, count], ...]` item stock; `gbd` carries the full list, and a skip is followed by the one item
  spent (`[["MS5",995]]`).

The window and the bot store these as they arrive (`direct/transfer.rs`, `observe`), each in its own place, and
never wait for one another. See [`../architecture/kingdom-transfer.md`](../architecture/kingdom-transfer.md).
