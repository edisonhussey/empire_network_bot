# `crst`: start a crafting recipe (refined wood, refined stone)

| | |
| --- | --- |
| Direction | client to server, answered by a `crst` reply |
| Evidence | captured (2026-11-16, us1, one account, two crafts) |
| Code | none in rust_distributable yet. A quick test is the **Test refine** button in proxy_packet_monitor (`web/app.js`, `runRefineTest`) |

## Purpose

Starts one craft in a castle's crafting building (the refinery). One `crst` is one craft: recipe 1 turns 54,400 wood into
100 refined wood, recipe 34 turns 54,400 stone into 100 refined stone. Each takes one hour. Repeat the packet to craft more.

## Request

```
%xt%EmpireEx_21%crst%1%{"KID":0,"AID":16011862,"OID":393,"PWR":0,"CRID":1}%
%xt%EmpireEx_21%crst%1%{"KID":0,"AID":16011862,"OID":393,"PWR":0,"CRID":34}%
```

| Field | Meaning |
| --- | --- |
| `KID` | kingdom of the castle (0 = Great Empire) |
| `AID` | the castle's area id (the same id as in `dcl`) |
| `OID` | the crafting building's object id in that castle, read from [`crin`](crin.md) (the `CBI` entry with `CQID` 1) |
| `PWR` | always 0 in the capture. Probably "pay with rubies" for the `Ruby` recipes (23, 56: 30 min, plus 3,200 rubies). Not tested |
| `CRID` | recipe id, `craftingRecipes` in the items file (`data/raw/game_files/rules/items/`) |

Recipes seen and their items-file rows (items v787.03):

| CRID | Reward | Cost | Duration | Queue |
| --- | --- | --- | --- | --- |
| 1 | 100 refined wood ("Refined Lumber", reward 40001) | 54,400 wood | 3,600 s | type 1 |
| 34 | 100 refined stone (reward 40034) | 54,400 stone | 3,600 s | type 1 |

Both use queue type 1, the building with `CQID` 1 (`WID` 2174 in the capture).

## Reply

The server answers each `crst` with four packets, always in this order: [`gcu`](gcu.md) (coins/rubies, unchanged), [`sce`](sce.md) (`[]`),
`grc` (the castle's new resources, already reduced by the cost) and the `crst` reply:

```
%xt%crst%1%0%{"KID":0,"AID":16011862,"OID":393,"CQID":1,"QS":{"CRID":[],"BV":[],"RUT":[]},"PS":{"CRID":[1],"BV":[0.0],"RUT":[],"RCT":[3600]},"S":4,"WID":2174}%
%xt%crst%1%0%{"KID":0,"AID":16011862,"OID":393,"CQID":1,"QS":{"CRID":[34],"BV":[0.0],"RUT":[]},"PS":{"CRID":[1],"BV":[0.0],"RUT":[],"RCT":[3599]},"S":4,"WID":2174}%
```

* `PS` is the production slot (what is crafting now): `CRID` the recipe, `RCT` the seconds left.
* `QS` is the waiting queue: the second craft went there (`CRID` [34]) because the one production slot was busy.
* Status `0` is success. Wood fell from 749,990.6 to 695,590.94 after recipe 1 and stone from 743,031.8 to 688,631.8 after
  recipe 34 (`grc`), exactly 54,400 each.

## Rules and behaviour

* Must be sent from castle view: the client sends it only there, after [`crin`](crin.md) and [`dcl`](dcl.md). Whether the server
  refuses it from the world map is not tested.
* One production slot and one queue slot by default (`craftingQueues`: `permanentProductionSlots` 1, `permanentQueueSlots` 1,
  more can be bought). So at most two crafts can be waiting per building: one crafting, one queued. Send one `crst` at a time
  and wait for its reply before the next (the client did, about 1.4 s apart).
* The client stays in castle view afterwards. No navigation packet follows.

## Captures

[`crst_refine.txt`](captures/crst_refine.txt) (both crafts with their replies).

## Open questions

* The status code when the queue is full or resources are short (not captured).
* What `BV` and `RUT` hold (always `0.0` and empty here), and the packet that collects a finished craft, if there is one.
* Whether `PWR` 1 pays rubies to skip.
