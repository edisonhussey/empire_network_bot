# `kpi`: Kingdom Info

| | |
| --- | --- |
| Direction | client to server, with a reply; also pushed, and nested in other packets |
| Evidence | proven live |
| Code | `empire-core/src/transfer.rs` (`find_kpi`, `parse_kpi`, `KingdomState`), `empire-daemon/src/direct/transfer.rs` (`observe`) |

## Purpose

The server's whole view of the kingdoms: which are unlocked, and what troops or goods are travelling between them. It is also inside `gbd` at login, inside [`kut`](kut.md) and [`msk`](msk.md) replies, and pushed when a transfer lands.

## Request

```
%xt%EmpireEx_21%kpi%1%{}%
```

## Reply

```
%xt%kpi%1%0%{"UL":[{"KID":0,"U":1,"C":0},{"KID":3,"U":1,"C":1,"SL":1,"SPW":81000,"SPS":81000,"SPF":0,"SPC1":120000},{"KID":4,"KRS":1280228,"CRS":2,"U":0,"C":0}],
              "UT":[{"KID":3,"RS":7200,"I":[[655,20]]}]}%
```

### `UL[]`: one row per kingdom (client `CastleKingdomVO.parseUnlockInfo`)

| Field | Meaning |
| --- | --- |
| `KID` | kingdom id: 0 green, 1 Burning Sands, 2 Everwinter, 3 Fire Peaks, 4 Storm Islands |
| `U` | `1` = unlocked for this account |
| `C` | has a Kontor (the kingdom's trading post); green shows `0` |
| `SL` | slum level (up to 4) |
| `SPW SPS SPF SPC1` | goods already paid toward the next slum level |
| `KRS`, `CRS` | seconds until the kingdom resets, and its reward set (kingdom 4) |

### `UT[]` and `RT[]`: what is travelling

`UT`: troops, `{"KID": target, "RS": seconds left, "I": [[unit, count], ...]}`. `RT`: goods, with `G` instead of `I`.

## Rules and behaviour

* **A snapshot, never a delta.** The client replaces its list on every `kpi`, so one without `UT` means nothing is travelling. Merging it would never notice a landing.
* `RS` counts from the packet's arrival; store `arrival + RS` as an absolute end time.
* The state is kept as it arrives, from any packet that carries it, and no step waits for it ([`../architecture/kingdom-transfer.md`](../architecture/kingdom-transfer.md)).

## Handling in the code

Stored per account in `app_state` (`transfer.kingdoms.<account>`); the transfer checks read it.

## Captures

[`kpi_request.txt`](captures/kpi_request.txt), [`kpi_response.txt`](captures/kpi_response.txt) (no `UT`), [`kut_response.txt`](captures/kut_response.txt) (a `kpi` with `UT`).

## Open questions

The meaning of `C` is read from the client's field name only.
