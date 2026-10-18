# The client code: where to look things up

`data/raw/game_files/client/` is the game's own program, exactly as served. It is the source of truth for **how the client
reads a packet and computes a number**: when a packet or a formula is unclear, read it here before guessing from a
capture. The files are minified JavaScript with the class and field names intact, so a plain text search works.

## The files

| File | Size | What it is |
| --- | ---: | --- |
| `Game.bundle.4230c06b48520e83ead4.js` | 15.9 MB | The game: every dialog, view-object (`...VO`), command handler (`...Command`) and request builder (`C2S...VO`) |
| `dll/ggs.dll.0b914fe686cb8e274f41.js` | 4.0 MB | The shared library: the **command-name tables** (`C2S_*` / `S2C_*` constants) and rule helpers such as `DungeonConst` and `TextValide` |
| `vendor.bundle.4aee9ab6.js` | 0.2 MB | Third-party libraries |
| `CacheBreaker.bundle.*.js`, `CastleLoader.bundle.*.js` | 0.04 MB, small | Start-up: the loader and cache-busting |
| `9b4320f03ea9e3eb7496.worker.js` | 0.01 MB | A web worker |
| `index.html`, `style.bundle.*.css`, `css/fonts_en.css` | small | The page that loads it all, and its styles |

The hashes in the names change with every client build; a new capture brings new ones.

## Lookups that have already paid off

| To learn | Search for | In | Gave |
| --- | --- | --- | --- |
| The wire name of a command | `C2S_KINGDOM_UNIT_TRANSFER` | `ggs.dll` (and `ClientConstSF` in `Game.bundle`) | `kut`; the same table names `msk`, `kst`, `kpi`, `acm`, `acl`, `aom`, ... |
| What a request carries | `function C2SKingdomUnitTransferVO` | `Game.bundle` | the field names and order: `SCID`, `SKID`, `TKID`, `CID`, `A` |
| How a reply is read | `function KUTCommand` (every `S2C` has an `...Command` with `executeCommand`) | `Game.bundle` | `kut` reads `gcu`, `gui` and `kpi` out of the reply |
| A price | `travelCostC1` | `Game.bundle` | `round(sum(kingdomTravellingCost * count * tax / 100))` |
| A robber baron's level | `DungeonConst` | `ggs.dll` | `floor(1.9 * count^0.555) + kingdom offset` |
| What a field means | `parseUnlockInfo`, `fillFromParamObject`, `ChatMessageVO` | `Game.bundle` | `kpi` `U`/`C`/`SL`; `UT` `RS`/`I`; chat `MA` is an age in seconds |
| Escaped text | `parseChatJSONMessage` | `ggs.dll` | the chat text escapes |

## How to search

From `data/raw/game_files/client/`: `grep -o ".\{200\}C2SKingdomUnitTransferVO.\{500\}" Game.bundle.*.js | head -3`
(or open it in an editor and search). Class names appear as `function ClassName(` and as `ClassName.prototype.method=`.
A command handler's `executeCommand(status, parts)` switch shows which status is OK (`ERROR.ALL_OK`) and what is parsed.

## Cautions

* It shows what the **client** does, not what the server enforces. A rule found here (a price, a limit) was confirmed
  against the server before the code relied on it ([`../problems/`](../problems/)).
* Minified code can place the same name in several places; read the one that defines it (`function X`) first.
