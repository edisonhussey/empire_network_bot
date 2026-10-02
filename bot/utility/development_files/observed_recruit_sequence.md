# Observed Sands recruitment sequence

Source: Ventrilo capture `gge_20261001_142750.log` and continuation at
`14:28:00`. Times below are local capture times on 2026-10-01.

## Confirmed commands

1. `14:27:52.454` — enter Sands castle:
   `jca {"CID":16366514,"KID":1}`. Server replied `jaa`, status `0`.
2. `14:27:55.223` — recruitment-page bootstrap:
   `dcl {"CD":0}`, `gpa {}`, `spl {"LID":0}`, `gui {}`.
   These appear to read page/state data rather than mutate it.
3. `14:27:58.653` — recruit 190 crossbowmen:
   `bup {"LID":0,"WID":607,"AMT":190,"PO":-1,"PWR":0,"SK":73,"SID":1,"AID":16366514}`.
4. The successful `bup` reply reported an active quantity of 5 and queued
   quantity of 185. `TCT=2850` is the authoritative total duration in seconds;
   `RCT=75` is the active batch's remaining duration.
5. `14:28:00.243` — request alliance help:
   `ahr {"ID":0,"T":6}`. The following `spl` state changed `RAH` to `true`.
6. `14:28:05.053` — open the main Green castle:
   `jca {"CID":16011862,"KID":0}`. Server replied `jaa`, status `0`.

No separate close-recruit-page command was observed before the final `jca`.
Closing the page is therefore represented as local navigation state, not a
made-up wire command.

## Noise / state broadcasts

- `pin` is heartbeat traffic.
- `gbl` and repeated `gaa` are map reads/refreshes and are not required to
  reproduce recruitment.
- General `ahh`, `ahd`, and `ahf` messages are alliance broadcasts. Only the
  `ahh` whose `OP` identifies castle `16366514`, kingdom `1`, lane `0` is tied
  to this recruitment request.
- `grc`, `gcu`, and `sce` are response/state traffic, not commands to replay.

## Later observations

- Controlled live trials later sent five 180-crossbowman `bup` requests in Fire
  and five in Sands. Every request received status `0` when the next request was
  held until the preceding response, and `TCT` accumulated across the lane.
- Alliance help succeeded after those slot sequences when castle context had
  first been established by a successful `jaa`.

## Still unproven

- `SK=73` is proven only for this crossbowman/castle/account combination.
- No universal inter-request delay has been established for other units,
  accounts, or recruitment lanes.
